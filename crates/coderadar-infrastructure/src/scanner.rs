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
        let root_path = canonical_root(root_dir)?;
        if mode == ScanMode::StagedDiff {
            return stream_staged(&root_path, sink);
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

/// Canonicalise a scan root and reject anything that is not a directory.
fn canonical_root(root_dir: &str) -> Result<std::path::PathBuf, DomainError> {
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
    Ok(root_path)
}

/// Stream only the files of the staged changeset (`PR-*` scans).
///
/// Deleted paths, paths that no longer exist, symlinks that resolve outside
/// the workspace and non-code files are skipped, exactly like the full walk.
fn stream_staged(root: &Path, sink: &(dyn ChunkSink + 'static)) -> Result<(), DomainError> {
    let changeset = crate::git_changeset::GitChangesetReader::read_staged(root)?;
    let mut chunker = Chunker::new();

    for file in &changeset.files {
        if file.path.split('/').any(|component| {
            component.starts_with('.') || IGNORED_DIRECTORY_NAMES.contains(&component)
        }) {
            continue;
        }
        let candidate = root.join(&file.path);
        let Ok(canonical) = std::fs::canonicalize(&candidate) else {
            // Deleted in the index: nothing to analyse.
            continue;
        };
        if !canonical.starts_with(root) || !canonical.is_file() {
            continue;
        }
        let Some(file_name) = canonical.file_name().and_then(OsStr::to_str) else {
            continue;
        };
        stream_file(&mut chunker, root, &canonical, file_name, sink)?;
    }

    Ok(())
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
    stream_file(chunker, root, entry.path(), file_name, sink)
}

/// Read, guard and chunk one file.
fn stream_file(
    chunker: &mut Chunker,
    root: &Path,
    path: &Path,
    file_name: &str,
    sink: &(dyn ChunkSink + 'static),
) -> Result<(), DomainError> {
    let Some(language) = detect_language(file_name) else {
        return Ok(());
    };
    let Ok(metadata) = std::fs::metadata(path) else {
        return Ok(());
    };
    if metadata.len() > MAX_FILE_BYTES {
        return Ok(());
    }
    let Ok(bytes) = std::fs::read(path) else {
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

    let file_path = relative_slash_path(root, path);
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
    use git2::{IndexAddOption, Repository, Signature};
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

    fn commit_all(repository: &Repository, message: &str) {
        let mut index = repository.index().expect("index");
        index
            .add_all(["."], IndexAddOption::DEFAULT, None)
            .expect("stage all");
        index.write().expect("write index");
        let tree_id = index.write_tree().expect("tree");
        let tree = repository.find_tree(tree_id).expect("tree object");
        let signature = Signature::now("CodeRadar Tests", "tests@coderadar.invalid").unwrap();
        let parents: Vec<git2::Commit<'_>> = match repository.head() {
            Ok(head) => vec![head.peel_to_commit().expect("commit")],
            Err(_) => Vec::new(),
        };
        let parent_refs: Vec<&git2::Commit<'_>> = parents.iter().collect();
        repository
            .commit(
                Some("HEAD"),
                &signature,
                &signature,
                message,
                &tree,
                &parent_refs,
            )
            .expect("commit");
    }

    fn stage(repository: &Repository, relative: &str) {
        let mut index = repository.index().expect("index");
        index.add_path(Path::new(relative)).expect("stage one path");
        index.write().expect("write index");
    }

    #[test]
    fn staged_diff_scans_only_the_staged_files() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        let repository = Repository::init(root).expect("repository");
        write_file(root, "src/a.ts", CLEAN_TS.as_bytes());
        write_file(root, "src/b.ts", b"export const b = 1;\n");
        commit_all(&repository, "initial");

        write_file(root, "src/a.ts", b"export const changed = 1;\n");
        write_file(root, "src/b.ts", b"export const changed = 2;\n");
        stage(&repository, "src/a.ts");

        let sink = CollectingChunkSink::new();
        ParallelFileScanner::new()
            .stream_project_chunks(root.to_str().unwrap(), ScanMode::StagedDiff, &sink)
            .expect("staged scan succeeds");
        let files: BTreeSet<String> = sink
            .drain()
            .iter()
            .map(|chunk| chunk.file_path.clone())
            .collect();
        assert_eq!(files, BTreeSet::from(["src/a.ts".to_string()]));
    }

    #[test]
    fn staged_diff_requires_a_git_repository() {
        let temp = TempDir::new().unwrap();
        write_file(temp.path(), "a.ts", CLEAN_TS.as_bytes());
        let sink = CollectingChunkSink::new();
        let error = ParallelFileScanner::new()
            .stream_project_chunks(temp.path().to_str().unwrap(), ScanMode::StagedDiff, &sink)
            .unwrap_err();
        assert!(matches!(error, DomainError::NotFound(_)), "{error:?}");
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
