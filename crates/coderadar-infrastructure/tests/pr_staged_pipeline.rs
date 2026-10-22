//! End-to-end staged-scan test (Phase 8): a real temporary Git repository
//! with staged changes must produce exactly the `PR-*` findings plus the
//! `pr_hygiene` scorecard block.

use std::fs;
use std::path::Path;

use coderadar_domain::ScanMode;
use coderadar_infrastructure::{scan_project_mode, ScanReport};
use git2::{IndexAddOption, Repository, Signature};
use tempfile::TempDir;

fn init_repository(root: &Path) -> Repository {
    let repository = Repository::init(root).expect("repository initialises");
    {
        let mut config = repository.config().expect("config");
        config.set_str("user.name", "CodeRadar Tests").unwrap();
        config
            .set_str("user.email", "tests@coderadar.invalid")
            .unwrap();
    }
    repository
}

fn write(root: &Path, relative: &str, contents: &str) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, contents).unwrap();
}

fn stage(repository: &Repository, relative: &str) {
    let mut index = repository.index().expect("index");
    index
        .add_all([relative], IndexAddOption::DEFAULT, None)
        .expect("stage");
    index.write().expect("write index");
}

fn commit(repository: &Repository, message: &str) {
    let mut index = repository.index().expect("index");
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

/// A production file with a couple of added lines, no test file touched.
///
/// The sources are deliberately clean so the Mock engine contributes no
/// findings: every finding in the report must come from the `PR-*` assessor.
#[test]
fn a_staged_scan_emits_the_expected_pr_findings_and_scorecard() {
    let directory = TempDir::new().expect("temp dir");
    let root = directory.path();
    let repository = init_repository(root);

    write(
        root,
        "src/lib.ts",
        "export function current(): number {\n  return 1;\n}\n",
    );
    write(root, "README.md", "# fixture\n");
    stage(&repository, "src/lib.ts");
    stage(&repository, "README.md");
    commit(&repository, "initial");

    write(
        root,
        "src/feature.ts",
        "export function current(): number {\n  return 1;\n}\n\nexport function growth(): number {\n  return 2;\n}\n",
    );
    stage(&repository, "src/feature.ts");

    let report: ScanReport =
        scan_project_mode(root, ScanMode::StagedDiff).expect("staged scan succeeds");

    // Exactly one finding: `PR-TST-003` anchored at the added production file.
    let triples: Vec<(String, String, u32)> = report
        .findings
        .iter()
        .map(|finding| {
            (
                finding.rule_id.clone(),
                finding.file.clone(),
                finding.line_start,
            )
        })
        .collect();
    assert_eq!(
        triples,
        vec![("PR-TST-003".to_string(), "src/feature.ts".to_string(), 1)],
        "unexpected finding set: {triples:?}"
    );

    let hygiene = report.pr_hygiene.as_ref().expect("scorecard is present");
    assert_eq!(hygiene.base, "HEAD");
    assert_eq!(hygiene.files_changed, 1);
    assert_eq!(hygiene.additions, 7);
    assert_eq!(hygiene.deletions, 0);
    assert_eq!(hygiene.lines_changed, 7);
    assert_eq!(hygiene.production_files_changed, 1);
    assert_eq!(hygiene.test_files_changed, 0);
    assert_eq!(hygiene.areas_touched, vec!["src"]);
    assert!(!hygiene.scope_creep);
    assert!(!hygiene.breaking_change);
    assert!(hygiene.missing_tests);

    assert_eq!(report.session.mode, "staged_diff");
    assert_eq!(report.scan.files_scanned, 1);
}

/// A full scan of the same repository must not carry a scorecard: the block
/// is present only for `staged_diff`.
#[test]
fn a_full_scan_carries_no_scorecard() {
    let directory = TempDir::new().expect("temp dir");
    let root = directory.path();
    let repository = init_repository(root);

    write(
        root,
        "src/lib.ts",
        "export function current(): number {\n  return 1;\n}\n",
    );
    stage(&repository, "src/lib.ts");
    commit(&repository, "initial");

    let report = scan_project_mode(root, ScanMode::FullScan).expect("full scan succeeds");
    assert!(report.pr_hygiene.is_none(), "{:#?}", report.pr_hygiene);
}
