//! Staged reports use index content even when the working copy diverges.
use std::fs;
use std::path::Path;

use coderadar_domain::ScanMode;
use coderadar_infrastructure::{
    scan_project_mode, CollectingChunkSink, GitChangesetReader, ParallelFileScanner,
};
use git2::{Repository, Signature};
use tempfile::TempDir;

#[test]
fn staged_ignore_rules_are_captured_from_index_not_worktree() {
    let directory = TempDir::new().unwrap();
    let root = directory.path();
    let repo = Repository::init(root).unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    for (path, contents) in [
        (".gitignore", "*.ts\n!src/\n"),
        ("src/.gitignore", "!keep.ts\n"),
        ("src/keep.ts", "export const keep = 1;\n"),
        ("src/drop.ts", "export const drop = 1;\n"),
    ] {
        fs::write(root.join(path), contents).unwrap();
    }
    let mut index = repo.index().unwrap();
    for path in [".gitignore", "src/.gitignore", "src/keep.ts", "src/drop.ts"] {
        index.add_path(Path::new(path)).unwrap();
    }
    index.write().unwrap();
    let snapshot = GitChangesetReader::capture_staged(root).unwrap();
    fs::write(root.join(".gitignore"), "src/keep.ts\n").unwrap();
    fs::write(root.join("src/.gitignore"), "drop.ts\n").unwrap();
    index.add_path(Path::new(".gitignore")).unwrap();
    index.add_path(Path::new("src/.gitignore")).unwrap();
    index.write().unwrap();
    fs::remove_file(root.join("src/keep.ts")).unwrap();
    fs::remove_file(root.join("src/drop.ts")).unwrap();

    let sink = CollectingChunkSink::new();
    let coverage = ParallelFileScanner::new()
        .stream_staged_snapshot_with_coverage(&snapshot, &sink)
        .unwrap();
    let paths: std::collections::BTreeSet<_> =
        sink.drain().into_iter().map(|c| c.file_path).collect();
    assert_eq!(
        paths,
        std::collections::BTreeSet::from(["src/keep.ts".to_string()])
    );
    assert_eq!(coverage.excluded, 3);
}

#[test]
fn ignored_parent_cannot_be_reincluded_by_nested_ignore_file() {
    let directory = TempDir::new().unwrap();
    let root = directory.path();
    let repo = Repository::init(root).unwrap();
    fs::create_dir_all(root.join("build")).unwrap();
    fs::write(root.join(".gitignore"), "build/\n").unwrap();
    fs::write(root.join("build/.gitignore"), "!keep.ts\n").unwrap();
    fs::write(root.join("build/keep.ts"), "export const keep = 1;\n").unwrap();
    let mut index = repo.index().unwrap();
    for path in [".gitignore", "build/.gitignore", "build/keep.ts"] {
        index.add_path(Path::new(path)).unwrap();
    }
    index.write().unwrap();
    let snapshot = GitChangesetReader::capture_staged(root).unwrap();
    let sink = CollectingChunkSink::new();
    ParallelFileScanner::new()
        .stream_staged_snapshot(&snapshot, &sink)
        .unwrap();
    assert!(sink.is_empty());
}

#[test]
fn unstaged_ignore_file_does_not_hide_indexed_source() {
    let directory = TempDir::new().unwrap();
    let root = directory.path();
    let repo = Repository::init(root).unwrap();
    fs::write(root.join("a.ts"), "export const a = 1;\n").unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(Path::new("a.ts")).unwrap();
    index.write().unwrap();
    fs::write(root.join(".gitignore"), "a.ts\n").unwrap();
    fs::remove_file(root.join("a.ts")).unwrap();
    let snapshot = GitChangesetReader::capture_staged(root).unwrap();
    let sink = CollectingChunkSink::new();
    ParallelFileScanner::new()
        .stream_staged_snapshot(&snapshot, &sink)
        .unwrap();
    assert!(sink.drain().iter().any(|chunk| chunk.file_path == "a.ts"));
}

#[test]
fn initial_index_survives_worktree_edits_and_deletion() {
    let directory = TempDir::new().unwrap();
    let root = directory.path();
    let repo = Repository::init(root).unwrap();
    let path = root.join("secrets.ts");
    fs::write(&path, "export const awsKey = \"AKIAIOSFODNN7EXAMPLE\";\n").unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(Path::new("secrets.ts")).unwrap();
    index.write().unwrap();

    let baseline = scan_project_mode(root, ScanMode::StagedDiff).unwrap();
    assert!(baseline.findings.iter().any(|f| f.rule_id == "SEC-TOK-002"));
    fs::write(&path, "export const safe = 1;\n").unwrap();
    let modified = scan_project_mode(root, ScanMode::StagedDiff).unwrap();
    fs::remove_file(&path).unwrap();
    let deleted = scan_project_mode(root, ScanMode::StagedDiff).unwrap();
    for report in [modified, deleted] {
        assert_eq!(report.scan.files_scanned, 1);
        assert!(report.findings.iter().any(|f| f.rule_id == "SEC-TOK-002"));
        assert_eq!(report.pr_hygiene.unwrap().additions, 1);
    }
}

#[test]
fn captured_snapshot_keeps_sources_and_hygiene_when_index_changes() {
    let directory = TempDir::new().unwrap();
    let root = directory.path();
    let repo = Repository::init(root).unwrap();
    fs::write(root.join("a.ts"), "export const a = 1;\n").unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(Path::new("a.ts")).unwrap();
    index.write().unwrap();
    let snapshot = GitChangesetReader::capture_staged(root).unwrap();
    fs::write(
        root.join("a.ts"),
        "export const b = 2;\nexport const c = 3;\n",
    )
    .unwrap();
    index.add_path(Path::new("a.ts")).unwrap();
    index.write().unwrap();

    let sink = CollectingChunkSink::new();
    ParallelFileScanner::new()
        .stream_staged_snapshot(&snapshot, &sink)
        .unwrap();
    let chunks = sink.drain();
    assert!(chunks
        .iter()
        .any(|c| c.content.contains("export const a = 1;")));
    assert!(chunks.iter().all(|c| !c.content.contains("export const b")));
    assert_eq!(snapshot.changeset().files[0].additions, 1);
    assert_eq!(
        snapshot.read_blob("a.ts", 100).unwrap().unwrap(),
        b"export const a = 1;\n"
    );
    assert!(snapshot.read_blob("../a.ts", 100).unwrap().is_none());
}

#[test]
fn staged_rename_and_deletion_use_new_index_entries() {
    let directory = TempDir::new().unwrap();
    let root = directory.path();
    let repo = Repository::init(root).unwrap();
    fs::write(
        root.join("old.ts"),
        "export const awsKey = \"AKIAIOSFODNN7EXAMPLE\";\n",
    )
    .unwrap();
    fs::write(root.join("removed.ts"), "export const removed = 1;\n").unwrap();
    let mut index = repo.index().unwrap();
    index
        .add_all(["*.ts"], git2::IndexAddOption::DEFAULT, None)
        .unwrap();
    let tree_id = index.write_tree().unwrap();
    index.write().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let signature = Signature::now("Test", "test@example.invalid").unwrap();
    repo.commit(Some("HEAD"), &signature, &signature, "initial", &tree, &[])
        .unwrap();
    fs::rename(root.join("old.ts"), root.join("new.ts")).unwrap();
    index.remove_path(Path::new("old.ts")).unwrap();
    index.remove_path(Path::new("removed.ts")).unwrap();
    index.add_path(Path::new("new.ts")).unwrap();
    index.write().unwrap();
    let before = fs::read(repo.path().join("index")).unwrap();
    fs::remove_file(root.join("new.ts")).unwrap();
    let report = scan_project_mode(root, ScanMode::StagedDiff).unwrap();
    assert_eq!(report.scan.files_scanned, 1);
    assert!(report
        .findings
        .iter()
        .any(|f| f.rule_id == "SEC-TOK-002" && f.file == "new.ts"));
    assert_eq!(fs::read(repo.path().join("index")).unwrap(), before);
}

#[test]
fn unmerged_index_is_reported_instead_of_scanning_a_mixed_snapshot() {
    let directory = TempDir::new().unwrap();
    let repo = Repository::init(directory.path()).unwrap();
    fs::write(
        directory.path().join("conflict.ts"),
        "export const a = 1;\n",
    )
    .unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(Path::new("conflict.ts")).unwrap();
    let mut entry = index.get_path(Path::new("conflict.ts"), 0).unwrap();
    index.remove_path(Path::new("conflict.ts")).unwrap();
    entry.flags = (entry.flags & !0x3000) | 0x2000;
    index.add(&entry).unwrap();
    index.write().unwrap();
    assert!(repo.index().unwrap().has_conflicts());
    let error = scan_project_mode(directory.path(), ScanMode::StagedDiff).unwrap_err();
    assert!(error.to_string().contains("unmerged"), "{error}");
}

#[test]
fn staged_symlink_and_hidden_entries_are_not_analyzed_as_source() {
    let directory = TempDir::new().unwrap();
    let root = directory.path();
    let repo = Repository::init(root).unwrap();
    fs::write(
        root.join("link.ts"),
        "export const awsKey = \"AKIAIOSFODNN7EXAMPLE\";\n",
    )
    .unwrap();
    fs::write(
        root.join(".hidden.ts"),
        "export const awsKey = \"AKIAIOSFODNN7EXAMPLE\";\n",
    )
    .unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(Path::new("link.ts")).unwrap();
    index.add_path(Path::new(".hidden.ts")).unwrap();
    let mut entry = index.get_path(Path::new("link.ts"), 0).unwrap();
    entry.mode = 0o120000;
    index.add(&entry).unwrap();
    index.write().unwrap();

    let snapshot = GitChangesetReader::capture_staged(root).unwrap();
    assert!(snapshot.read_blob("link.ts", 1024).unwrap().is_none());
    let report = scan_project_mode(root, ScanMode::StagedDiff).unwrap();
    assert_eq!(report.scan.files_scanned, 0);
    assert!(!report.findings.iter().any(|f| f.rule_id == "SEC-TOK-002"));
}
