//! Domain invariants of the Git changeset and the `PR-*` hygiene rules.

use coderadar_domain::{
    is_public_api_removal, is_test_path, Changeset, FileChange, SCOPE_CREEP_LINE_THRESHOLD,
};

fn large_changeset() -> Changeset {
    Changeset::new(
        "HEAD",
        vec![
            FileChange::new("src/a.ts", 400, 150),
            FileChange::new("docs/readme.md", 10, 0),
        ],
    )
}

#[test]
fn test_paths_cover_the_common_conventions() {
    for path in [
        "tests/scan_pipeline.rs",
        "src/a.test.ts",
        "src/a.spec.ts",
        "src/__tests__/a.ts",
        "crates/x/src/test_helpers.py",
        "pkg/handler_test.go",
        "src/main_test.rs",
    ] {
        assert!(is_test_path(path), "{path} must be a test path");
    }
    for path in [
        "src/main.ts",
        "crates/coderadar-domain/src/lib.rs",
        "README.md",
        "src/latest.ts",
    ] {
        assert!(!is_test_path(path), "{path} must not be a test path");
    }
}

#[test]
fn public_api_removal_is_conservative() {
    for line in [
        "export function parse(input: string): Token",
        "export const API_URL = \"https://example.com\";",
        "pub fn scan(root: &Path) -> Report;",
        "public class UserController {",
        "protected void refresh() {",
    ] {
        assert!(is_public_api_removal(line), "{line} must count as public");
    }
    for line in [
        "const helper = 1;",
        "  function inner() {",
        "// export this later",
        "secret = \"sk-live\"",
    ] {
        assert!(
            !is_public_api_removal(line),
            "{line} must not count as public"
        );
    }
}

#[test]
fn metrics_are_deterministic_and_sum_the_diff() {
    let changeset = large_changeset();
    assert_eq!(changeset.files_changed(), 2);
    assert_eq!(changeset.additions(), 410);
    assert_eq!(changeset.deletions(), 150);
    assert_eq!(changeset.lines_changed(), 560);
    assert_eq!(changeset.areas(), vec!["docs", "src"]);
    assert_eq!(changeset.production_code_files_changed(), 1);
    assert_eq!(changeset.test_files_changed(), 0);
}

#[test]
fn a_large_diff_across_two_areas_is_scope_creep() {
    let hygiene = large_changeset().assess();
    assert!(hygiene.scope_creep);
    assert!(hygiene
        .violations
        .iter()
        .any(|violation| violation.rule_id == "PR-SCP-001"));
    // The anchor is the first production *code* file, not the docs file.
    let violation = hygiene
        .violations
        .iter()
        .find(|violation| violation.rule_id == "PR-SCP-001")
        .expect("scope creep violation");
    assert_eq!(violation.file_path, "src/a.ts");
}

#[test]
fn a_large_single_area_diff_is_not_scope_creep() {
    let changeset = Changeset::new(
        "HEAD",
        vec![FileChange::new(
            "src/a.ts",
            SCOPE_CREEP_LINE_THRESHOLD + 1,
            0,
        )],
    );
    assert!(!changeset.assess().scope_creep);
}

#[test]
fn exactly_the_threshold_is_not_scope_creep() {
    let changeset = Changeset::new(
        "HEAD",
        vec![
            FileChange::new("src/a.ts", SCOPE_CREEP_LINE_THRESHOLD - 10, 0),
            FileChange::new("docs/a.md", 10, 0),
        ],
    );
    let hygiene = changeset.assess();
    assert_eq!(hygiene.lines_changed, SCOPE_CREEP_LINE_THRESHOLD);
    assert!(!hygiene.scope_creep, "the threshold is exclusive");
}

#[test]
fn a_removed_public_signature_is_a_breaking_change_per_file() {
    let changeset = Changeset::new(
        "HEAD",
        vec![
            FileChange::new("src/api.ts", 0, 4)
                .with_removed_public_api(vec!["export function oldApi() {}".to_string()]),
            FileChange::new("src/other.ts", 0, 1).with_removed_public_api(vec![
                "pub fn old(a: u8)".to_string(),
                "pub struct Old;".to_string(),
            ]),
            FileChange::new("src/private.ts", 0, 2),
        ],
    );
    let hygiene = changeset.assess();
    assert!(hygiene.breaking_change);
    let violations: Vec<_> = hygiene
        .violations
        .iter()
        .filter(|violation| violation.rule_id == "PR-BRK-002")
        .collect();
    assert_eq!(violations.len(), 2, "one violation per affected file");
    assert!(violations
        .iter()
        .any(|violation| violation.file_path == "src/api.ts"));
    assert!(violations
        .iter()
        .any(|violation| violation.file_path == "src/other.ts"));
    for violation in violations {
        assert!(
            !violation.description.contains("oldApi"),
            "violations must not echo source text"
        );
    }
}

#[test]
fn production_changes_without_tests_are_flagged() {
    let changeset = Changeset::new("HEAD", vec![FileChange::new("src/a.ts", 12, 2)]);
    let hygiene = changeset.assess();
    assert!(hygiene.missing_tests);
    assert!(hygiene
        .violations
        .iter()
        .any(|violation| violation.rule_id == "PR-TST-003"));
}

#[test]
fn production_and_test_changes_are_not_flagged() {
    let changeset = Changeset::new(
        "HEAD",
        vec![
            FileChange::new("src/a.ts", 12, 2),
            FileChange::new("tests/a.test.ts", 8, 0),
        ],
    );
    let hygiene = changeset.assess();
    assert!(!hygiene.missing_tests);
    assert_eq!(hygiene.test_files_changed, 1);
}

#[test]
fn docs_only_changes_do_not_require_tests() {
    let changeset = Changeset::new(
        "HEAD",
        vec![
            FileChange::new("docs/README.md", 10, 1),
            FileChange::new("Cargo.lock", 3, 0),
        ],
    );
    let hygiene = changeset.assess();
    assert!(!hygiene.missing_tests);
}

#[test]
fn a_clean_small_changeset_has_no_violations() {
    let changeset = Changeset::new(
        "HEAD",
        vec![
            FileChange::new("src/a.ts", 12, 2),
            FileChange::new("tests/a.test.ts", 8, 0),
        ],
    );
    let hygiene = changeset.assess();
    assert!(hygiene.is_clean(), "{hygiene:#?}");
    assert_eq!(hygiene.areas_touched, vec!["src", "tests"]);
    assert!(!hygiene.scope_creep);
    assert!(!hygiene.breaking_change);
    assert!(!hygiene.missing_tests);
}

#[test]
fn violations_are_sorted_by_rule_then_file() {
    let changeset = Changeset::new(
        "HEAD",
        vec![
            FileChange::new("src/b.ts", 600, 0)
                .with_removed_public_api(vec!["export const b = 1;".to_string()]),
            FileChange::new("src/a.ts", 600, 0)
                .with_removed_public_api(vec!["export const a = 1;".to_string()]),
        ],
    );
    let hygiene = changeset.assess();
    let keys: Vec<(&str, &str)> = hygiene
        .violations
        .iter()
        .map(|violation| (violation.rule_id, violation.file_path.as_str()))
        .collect();
    let mut sorted = keys.clone();
    sorted.sort_unstable();
    assert_eq!(keys, sorted);
}
