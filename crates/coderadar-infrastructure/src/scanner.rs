//! Parallel project scanner built on `ignore::WalkParallel`.
//!
//! Implements the Layer 2 [`ProjectScannerGateway`] port. The walk honours
//! `.gitignore` (the `ignore` crate) and the CodeRadar bloat guard:
//!
//! * hidden entries (dot-prefixed names) are skipped, matching the `ignore`
//!   crate and common linter defaults — generated dot-directories such as
//!   `.next` or `.yarn` are therefore never scanned;
//! * directories `node_modules`, `vendor`, `target` are never entered;
//! * files larger than [`MAX_FILE_BYTES`] (1 MB) are skipped;
//! * files with any line longer than [`MAX_LINE_LENGTH`] characters (1,000)
//!   are skipped — this catches minified/bundled output;
//! * files that are not valid UTF-8 are skipped.
//!
//! For every surviving TypeScript/JavaScript file the scanner emits one
//! [`coderadar_domain::ChunkKind::FileModule`] chunk carrying the full source
//! (file/module-scope Tier 0 rules) followed by its
//! [`coderadar_domain::ChunkKind::AstWindow`] chunks from the tree-sitter
//! chunker.
//!
//! The walk runs on multiple worker threads; each worker owns its own
//! [`Chunker`], so the (not `Sync`) tree-sitter parser never crosses a thread
//! boundary. Chunks flow through the shared [`ChunkSink`] as soon as a file is
//! read, so callers do not have to hold the whole project in memory.

use std::ffi::OsStr;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use coderadar_domain::{CodeChunk, DomainError, ScanMode};
use coderadar_usecases::{ChunkSink, ProjectScannerGateway};
use ignore::{DirEntry, WalkBuilder, WalkState};

use crate::chunker::{detect_language, Chunker};

/// Maximum size of a scanned file (1 MB).
pub const MAX_FILE_BYTES: u64 = 1024 * 1024;

/// Maximum length of a single line (characters) before the file is skipped.
pub const MAX_LINE_LENGTH: usize = 1000;

/// Directory names that are never entered, regardless of `.gitignore`.
pub const IGNORED_DIRECTORY_NAMES: [&str; 3] = ["node_modules", "vendor", "target"];

/// A parallel, `.gitignore`-aware file scanner.
#[derive(Debug, Default)]
pub struct ParallelFileScanner;

impl ParallelFileScanner {
    /// Create a scanner.
    pub fn new() -> Self {
        Self
    }
}

impl ProjectScannerGateway for ParallelFileScanner {
    fn stream_project_chunks(
        &self,
        root_dir: &str,
        mode: ScanMode,
        sink: &(dyn ChunkSink + 'static),
    ) -> Result<(), DomainError> {
        match mode {
            ScanMode::FullScan => {}
            ScanMode::StagedDiff => {
                return Err(DomainError::External(
                    "staged diff scanning arrives with the Phase 8 Git engine".to_string(),
                ));
            }
        }

        let root_path = std::fs::canonicalize(root_dir).map_err(|error| {
            DomainError::NotFound(format!(
                "scan root '{root_dir}' cannot be canonicalized: {error}"
            ))
        })?;
        if !root_path.is_dir() {
            return Err(DomainError::NotFound(format!(
                "scan root '{root_dir}' is not a directory"
            )));
        }

        let root = root_path.as_path();
        let failed = AtomicBool::new(false);
        let failed_flag = &failed;

        let mut builder = WalkBuilder::new(root);
        builder.require_git(false).filter_entry(|entry| {
            entry.file_type().is_some_and(|file_type| {
                !file_type.is_dir()
                    || !IGNORED_DIRECTORY_NAMES
                        .iter()
                        .any(|ignored| entry.file_name() == OsStr::new(ignored))
            })
        });
        let walker = builder.build_parallel();
        walker.run(|| {
            let mut chunker = Chunker::new();
            Box::new(move |entry| {
                let entry = match entry {
                    Ok(entry) => entry,
                    Err(_) => return WalkState::Continue,
                };
                if stream_entry(&mut chunker, root, &entry, sink).is_err() {
                    failed_flag.store(true, Ordering::Relaxed);
                }
                WalkState::Continue
            })
        });

        if failed.load(Ordering::Relaxed) {
            return Err(DomainError::External(
                "chunk sink rejected chunks".to_string(),
            ));
        }
        Ok(())
    }
}

/// Stream the chunks of one walked entry.
///
/// Entries that are not scannable — directories, symlinks, unsupported
/// extensions, oversized files, invalid UTF-8, minified lines — are skipped
/// silently, as are files that cannot be read: one bad file never aborts the
/// walk. The only error that propagates is the sink rejecting a chunk.
fn stream_entry(
    chunker: &mut Chunker,
    root: &Path,
    entry: &DirEntry,
    sink: &(dyn ChunkSink + 'static),
) -> Result<(), DomainError> {
    let Some(file_type) = entry.file_type() else {
        return Ok(());
    };
    if !file_type.is_file() {
        return Ok(());
    }
    let Some(file_name) = entry.file_name().to_str() else {
        return Ok(());
    };
    let Some(language) = detect_language(file_name) else {
        return Ok(());
    };
    let Ok(metadata) = entry.metadata() else {
        return Ok(());
    };
    if metadata.len() > MAX_FILE_BYTES {
        return Ok(());
    }
    let Ok(bytes) = std::fs::read(entry.path()) else {
        return Ok(());
    };
    let Ok(source) = String::from_utf8(bytes) else {
        return Ok(());
    };
    if source
        .lines()
        .any(|line| line.chars().count() > MAX_LINE_LENGTH)
    {
        return Ok(());
    }

    let file_path = relative_slash_path(root, entry.path());
    let line_count = source.lines().count() as u32;
    sink.send_chunk(CodeChunk::file_module(
        &file_path, language, line_count, &source,
    ))?;
    for chunk in chunker.chunk(&file_path, language, &source) {
        sink.send_chunk(chunk)?;
    }
    Ok(())
}

/// Slash-separated path of `path` relative to the canonical scan `root`.
fn relative_slash_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::fs;
    use std::path::Path;

    use coderadar_domain::{ChunkKind, CodeChunk, DomainError, ScanMode};
    use coderadar_usecases::{ChunkSink, ProjectScannerGateway};
    use tempfile::TempDir;

    use super::{ParallelFileScanner, MAX_FILE_BYTES, MAX_LINE_LENGTH};
    use crate::sinks::CollectingChunkSink;

    const CLEAN_TS: &str = "export function alpha(): number {\n  return 1;\n}\n\nexport function beta(): number {\n  return alpha() + 1;\n}\n";

    fn write_file(root: &Path, relative: &str, contents: &[u8]) {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, contents).unwrap();
    }

    fn build_tree(root: &Path) {
        write_file(root, "a.ts", CLEAN_TS.as_bytes());
        write_file(root, "b.ts", b"export const ignoredByGitignore = 1;\n");
        write_file(root, ".gitignore", b"b.ts\n");
        write_file(
            root,
            "node_modules/dep.ts",
            b"export const dependency = 1;\n",
        );
        write_file(root, "skip.py", b"def add(a, b):\n    return a + b\n");
        write_file(root, "blob.ts", &[0xFF, 0xFE, 0xFD, 0xFC, b'\n']);
        let long_line = format!(
            "export const long = \"{}\";\n",
            "x".repeat(MAX_LINE_LENGTH + 500)
        );
        write_file(root, "long.ts", long_line.as_bytes());
        let mut big = String::new();
        for _ in 0..1200 {
            big.push_str(&"y".repeat(900));
            big.push('\n');
        }
        assert!(big.len() as u64 > MAX_FILE_BYTES);
        write_file(root, "big.ts", big.as_bytes());
    }

    fn scan(root: &Path, sink: &CollectingChunkSink) {
        ParallelFileScanner::new()
            .stream_project_chunks(root.to_str().unwrap(), ScanMode::FullScan, sink)
            .unwrap();
    }

    #[test]
    fn full_scan_streams_expected_files_only() {
        let temp = TempDir::new().unwrap();
        build_tree(temp.path());

        let sink = CollectingChunkSink::new();
        scan(temp.path(), &sink);
        let chunks = sink.drain();

        let files: BTreeSet<&str> = chunks
            .iter()
            .map(|chunk| chunk.file_path.as_str())
            .collect();
        assert_eq!(files, BTreeSet::from(["a.ts"]));

        for chunk in &chunks {
            assert!(!chunk.file_path.contains('\\'), "{}", chunk.file_path);
            assert!(Path::new(&chunk.file_path).is_relative());
            assert_eq!(chunk.language, "typescript");
        }

        let modules: Vec<&CodeChunk> = chunks
            .iter()
            .filter(|chunk| chunk.kind == ChunkKind::FileModule)
            .collect();
        let windows: Vec<&CodeChunk> = chunks
            .iter()
            .filter(|chunk| chunk.kind == ChunkKind::AstWindow)
            .collect();
        assert_eq!(modules.len(), 1);
        assert!(!windows.is_empty());

        let module = modules[0];
        assert_eq!(module.start_line, 1);
        assert_eq!(module.end_line, 7);
        assert_eq!(module.content, CLEAN_TS);

        let round_trip: String = windows.iter().map(|chunk| chunk.content.as_str()).collect();
        assert_eq!(round_trip, CLEAN_TS);
    }

    #[test]
    fn hidden_entries_are_skipped_by_design() {
        let temp = TempDir::new().unwrap();
        write_file(temp.path(), "visible.ts", CLEAN_TS.as_bytes());
        write_file(
            temp.path(),
            ".eslintrc.js",
            b"module.exports = { token: \"AKIAIOSFODNN7EXAMPLE\" };\n",
        );
        write_file(
            temp.path(),
            ".storybook/config.js",
            b"export const base = \"https://example.com\";\n",
        );

        let sink = CollectingChunkSink::new();
        scan(temp.path(), &sink);
        let files: BTreeSet<String> = sink
            .drain()
            .iter()
            .map(|chunk| chunk.file_path.clone())
            .collect();
        assert_eq!(files, BTreeSet::from(["visible.ts".to_string()]));
    }

    #[test]
    fn staged_diff_mode_is_not_implemented_yet() {
        let temp = TempDir::new().unwrap();
        let sink = CollectingChunkSink::new();
        let error = ParallelFileScanner::new()
            .stream_project_chunks(temp.path().to_str().unwrap(), ScanMode::StagedDiff, &sink)
            .unwrap_err();
        assert!(matches!(&error, DomainError::External(message) if message.contains("Phase 8")));
        assert!(sink.is_empty());
    }

    #[test]
    fn missing_root_is_reported_as_not_found() {
        let temp = TempDir::new().unwrap();
        let missing = temp.path().join("missing");
        let sink = CollectingChunkSink::new();
        let error = ParallelFileScanner::new()
            .stream_project_chunks(missing.to_str().unwrap(), ScanMode::FullScan, &sink)
            .unwrap_err();
        assert!(matches!(error, DomainError::NotFound(_)));
    }

    #[test]
    fn file_instead_of_directory_root_is_rejected() {
        let temp = TempDir::new().unwrap();
        let file = temp.path().join("a.ts");
        fs::write(&file, "export const a = 1;\n").unwrap();
        let sink = CollectingChunkSink::new();
        let error = ParallelFileScanner::new()
            .stream_project_chunks(file.to_str().unwrap(), ScanMode::FullScan, &sink)
            .unwrap_err();
        assert!(matches!(error, DomainError::NotFound(_)));
    }

    struct RejectingChunkSink;

    impl ChunkSink for RejectingChunkSink {
        fn send_chunk(&self, _chunk: CodeChunk) -> Result<(), DomainError> {
            Err(DomainError::External("sink is closed".to_string()))
        }
    }

    #[test]
    fn sink_failure_is_reported_after_the_walk() {
        let temp = TempDir::new().unwrap();
        write_file(temp.path(), "a.ts", CLEAN_TS.as_bytes());

        let error = ParallelFileScanner::new()
            .stream_project_chunks(
                temp.path().to_str().unwrap(),
                ScanMode::FullScan,
                &RejectingChunkSink,
            )
            .unwrap_err();
        assert!(matches!(&error, DomainError::External(message) if message.contains("rejected")));
    }
}
