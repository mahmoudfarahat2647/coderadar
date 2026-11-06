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
use std::io::Read;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use coderadar_domain::{CodeChunk, DomainError, ScanCoverage, ScanMode};
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

    /// Stream changed files from an already captured index snapshot.
    ///
    /// # Errors
    /// Returns an error if a captured blob cannot be read or the sink rejects a chunk.
    pub fn stream_staged_snapshot(
        &self,
        snapshot: &crate::git_changeset::StagedSnapshot,
        sink: &(dyn ChunkSink + 'static),
    ) -> Result<(), DomainError> {
        self.stream_staged_snapshot_with_coverage(snapshot, sink)
            .map(|_| ())
    }

    /// Stream a captured snapshot and return file-level coverage.
    ///
    /// # Errors
    /// Returns an error if the sink rejects a chunk.
    pub fn stream_staged_snapshot_with_coverage(
        &self,
        snapshot: &crate::git_changeset::StagedSnapshot,
        sink: &(dyn ChunkSink + 'static),
    ) -> Result<ScanCoverage, DomainError> {
        stream_snapshot(snapshot, sink)
    }

    /// Stream a full project and return file-level coverage.
    ///
    /// # Errors
    /// Returns an error if the root is invalid or the sink rejects a chunk.
    pub fn stream_project_with_coverage(
        &self,
        root_dir: &str,
        mode: ScanMode,
        sink: &(dyn ChunkSink + 'static),
    ) -> Result<ScanCoverage, DomainError> {
        let root_path = canonical_root(root_dir)?;
        if mode == ScanMode::StagedDiff {
            let snapshot = crate::git_changeset::GitChangesetReader::capture_staged(&root_path)?;
            return stream_snapshot(&snapshot, sink);
        }

        let root = root_path.as_path();
        let failed = AtomicBool::new(false);
        let coverage = Arc::new(Mutex::new(ScanCoverage {
            exclusions_complete: Some(false),
            ..ScanCoverage::default()
        }));
        let filter_coverage = Arc::clone(&coverage);
        let mut builder = WalkBuilder::new(root);
        builder
            .require_git(false)
            .hidden(false)
            .filter_entry(move |entry| {
                let name = entry.file_name();
                let excluded = entry.depth() > 0
                    && (name.to_string_lossy().starts_with('.')
                        || (entry.file_type().is_some_and(|kind| kind.is_dir())
                            && IGNORED_DIRECTORY_NAMES
                                .iter()
                                .any(|ignored| name == OsStr::new(ignored))));
                if excluded {
                    filter_coverage.lock().unwrap().excluded += 1;
                }
                !excluded
            });
        let walker = builder.build_parallel();
        walker.run(|| {
            let mut chunker = Chunker::new();
            let coverage = &coverage;
            let failed = &failed;
            Box::new(move |entry| {
                match entry {
                    Ok(entry) => {
                        if stream_entry(&mut chunker, root, &entry, sink, coverage).is_err() {
                            failed.store(true, Ordering::Relaxed);
                        }
                    }
                    Err(_) => coverage.lock().unwrap().unreadable += 1,
                }
                WalkState::Continue
            })
        });
        if failed.load(Ordering::Relaxed) {
            return Err(DomainError::External(
                "chunk sink rejected chunks".to_string(),
            ));
        }
        let result = *coverage.lock().unwrap();
        Ok(result)
    }
}

impl ProjectScannerGateway for ParallelFileScanner {
    fn stream_project_chunks(
        &self,
        root_dir: &str,
        mode: ScanMode,
        sink: &(dyn ChunkSink + 'static),
    ) -> Result<(), DomainError> {
        self.stream_project_with_coverage(root_dir, mode, sink)
            .map(|_| ())
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

fn stream_snapshot(
    snapshot: &crate::git_changeset::StagedSnapshot,
    sink: &(dyn ChunkSink + 'static),
) -> Result<ScanCoverage, DomainError> {
    let mut chunker = Chunker::new();
    let mut coverage = ScanCoverage {
        exclusions_complete: Some(true),
        ..ScanCoverage::default()
    };

    for file in &snapshot.changeset().files {
        if snapshot.is_ignored(&file.path)
            || file.path.split('/').any(|component| {
                component.starts_with('.') || IGNORED_DIRECTORY_NAMES.contains(&component)
            })
        {
            coverage.excluded += 1;
            continue;
        }
        if snapshot.blob_id(&file.path).is_none() {
            coverage.excluded += 1;
            continue;
        }
        coverage.discovered += 1;
        let Some(file_name) = file.path.rsplit('/').next() else {
            continue;
        };
        let Some(language) = detect_language(file_name) else {
            coverage.unsupported += 1;
            continue;
        };
        coverage.supported += 1;
        let bytes = match snapshot.read_blob(&file.path, MAX_FILE_BYTES) {
            Ok(Some(bytes)) => bytes,
            Ok(None) => {
                coverage.oversized += 1;
                continue;
            }
            Err(_) => {
                coverage.unreadable += 1;
                continue;
            }
        };
        let Ok(source) = String::from_utf8(bytes) else {
            coverage.invalid_encoding += 1;
            continue;
        };
        if too_long(&source) {
            coverage.oversized += 1;
            continue;
        }
        stream_source(&mut chunker, &file.path, language, &source, sink)?;
        coverage.incomplete += 1;
    }

    Ok(coverage)
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
    coverage: &Mutex<ScanCoverage>,
) -> Result<(), DomainError> {
    let Some(file_type) = entry.file_type() else {
        return Ok(());
    };
    if !file_type.is_file() {
        return Ok(());
    }
    let file_name = entry.file_name().to_string_lossy();
    stream_file(chunker, root, entry.path(), &file_name, sink, coverage)
}

/// Read, guard and chunk one file.
fn stream_file(
    chunker: &mut Chunker,
    root: &Path,
    path: &Path,
    file_name: &str,
    sink: &(dyn ChunkSink + 'static),
    coverage: &Mutex<ScanCoverage>,
) -> Result<(), DomainError> {
    coverage.lock().unwrap().discovered += 1;
    let Some(language) = detect_language(file_name) else {
        coverage.lock().unwrap().unsupported += 1;
        return Ok(());
    };
    coverage.lock().unwrap().supported += 1;
    let Ok(file) = std::fs::File::open(path) else {
        coverage.lock().unwrap().unreadable += 1;
        return Ok(());
    };
    let mut bytes = Vec::new();
    if file
        .take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .is_err()
    {
        coverage.lock().unwrap().unreadable += 1;
        return Ok(());
    }
    if bytes.len() as u64 > MAX_FILE_BYTES {
        coverage.lock().unwrap().oversized += 1;
        return Ok(());
    }
    let Ok(source) = String::from_utf8(bytes) else {
        coverage.lock().unwrap().invalid_encoding += 1;
        return Ok(());
    };
    if too_long(&source) {
        coverage.lock().unwrap().oversized += 1;
        return Ok(());
    }
    let file_path = relative_slash_path(root, path);
    stream_source(chunker, &file_path, language, &source, sink)?;
    coverage.lock().unwrap().incomplete += 1;
    Ok(())
}

fn too_long(source: &str) -> bool {
    source
        .lines()
        .any(|line| line.chars().count() > MAX_LINE_LENGTH)
}

fn stream_source(
    chunker: &mut Chunker,
    file_path: &str,
    language: &str,
    source: &str,
    sink: &(dyn ChunkSink + 'static),
) -> Result<(), DomainError> {
    let line_count = source.lines().count() as u32;
    sink.send_chunk(CodeChunk::file_module(
        file_path, language, line_count, source,
    ))?;
    for chunk in chunker.chunk(file_path, language, source) {
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
    fn full_scan_counts_only_observed_exclusions_without_descending() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        write_file(root, ".gitignore", b"*.ts\n!keep.ts\nignored/\n");
        write_file(root, "keep.ts", CLEAN_TS.as_bytes());
        write_file(root, "drop.ts", CLEAN_TS.as_bytes());
        write_file(root, "ignored/a.ts", CLEAN_TS.as_bytes());
        write_file(root, "ignored/nested/b.ts", CLEAN_TS.as_bytes());
        let sink = CollectingChunkSink::new();
        let coverage = ParallelFileScanner::new()
            .stream_project_with_coverage(root.to_str().unwrap(), ScanMode::FullScan, &sink)
            .unwrap();
        assert_eq!(coverage.discovered, 1);
        assert_eq!(coverage.excluded, 1); // Only .gitignore was observed by the filter.
        let files: BTreeSet<String> = sink.drain().into_iter().map(|c| c.file_path).collect();
        assert_eq!(files, BTreeSet::from(["keep.ts".to_string()]));
    }

    #[test]
    fn full_scan_does_not_infer_nested_ignore_exclusions() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        write_file(
            root,
            ".gitignore",
            b"*.ts\n!src/\nnode_modules/\n.hidden.ts\n",
        );
        write_file(root, "src/.gitignore", b"!keep.ts\n");
        write_file(root, "src/keep.ts", CLEAN_TS.as_bytes());
        write_file(root, "src/drop.ts", CLEAN_TS.as_bytes());
        write_file(root, "node_modules/dep.ts", CLEAN_TS.as_bytes());
        write_file(root, ".hidden.ts", CLEAN_TS.as_bytes());
        let sink = CollectingChunkSink::new();
        let coverage = ParallelFileScanner::new()
            .stream_project_with_coverage(root.to_str().unwrap(), ScanMode::FullScan, &sink)
            .unwrap();
        assert_eq!(coverage.discovered, 1);
        assert_eq!(coverage.excluded, 2); // The two .gitignore files were observed.
        let files: BTreeSet<String> = sink.drain().into_iter().map(|c| c.file_path).collect();
        assert_eq!(files, BTreeSet::from(["src/keep.ts".to_string()]));
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
