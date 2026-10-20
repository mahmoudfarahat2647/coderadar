//! Domain tests for the patch model and the compliance score.

use coderadar_domain::{
    apply_edits, DomainError, Finding, LineEdit, PatchOutcome, PatchProposal, ScanMode,
    ScanSession, Severity,
};

#[test]
fn an_edit_replaces_its_byte_range() {
    let content = "const decoded = jwt.decode(token);\n";
    let edit = LineEdit::new(
        1,
        1,
        "const decoded = ".len(),
        "const decoded = jwt.decode(token)".len(),
        "jwt.verify(token)",
    );
    let patched = apply_edits(content, &[edit]).expect("the edit applies");
    assert_eq!(patched, "const decoded = jwt.verify(token);\n");
}

#[test]
fn several_edits_are_applied_together() {
    let content = "aaa\nbbb\nccc\n";
    let edits = vec![
        LineEdit::new(1, 1, 0, 3, "AAA"),
        LineEdit::new(3, 3, 8, 11, "CCC"),
    ];
    let patched = apply_edits(content, &edits).expect("the edits apply");
    assert_eq!(patched, "AAA\nbbb\nCCC\n");
}

#[test]
fn overlapping_edits_are_rejected() {
    let content = "aaa\nbbb\n";
    let edits = vec![
        LineEdit::new(1, 2, 0, 7, "x"),
        LineEdit::new(1, 1, 0, 3, "y"),
    ];
    let error = apply_edits(content, &edits).expect_err("overlaps are refused");
    assert!(error.to_string().contains("overlapping"), "{error}");
}

#[test]
fn an_edit_outside_the_content_is_rejected() {
    let content = "aaa\n";
    let edits = vec![LineEdit::new(1, 1, 0, 99, "x")];
    assert!(apply_edits(content, &edits).is_err());
}

#[test]
fn an_edit_covering_a_multibyte_character_applies() {
    let content = "let greeting = \"ééé\";\n";
    let start = content.find('é').expect("the accented character is there");
    let end = content.rfind('é').expect("the accented character is there") + 'é'.len_utf8();
    let edits = vec![LineEdit::new(1, 1, start, end, "üüü")];
    let patched = apply_edits(content, &edits).expect("the edit applies");
    assert_eq!(patched, "let greeting = \"üüü\";\n");
}

#[test]
fn an_edit_starting_inside_a_multibyte_character_is_rejected() {
    let content = "let greeting = \"ééé\";\n";
    let start = content.find('é').expect("the accented character is there");
    // One byte inside the two-byte `é`: not a character boundary.
    let edits = vec![LineEdit::new(1, 1, start + 1, start + 2, "x")];
    let error = apply_edits(content, &edits).expect_err("a split character is refused");
    assert!(matches!(error, DomainError::Invariant(_)), "{error:?}");
    assert!(error.to_string().contains("character boundary"), "{error}");
}

#[test]
fn an_edit_ending_inside_an_emoji_is_rejected() {
    let content = "const emoji = \"😀\";\n";
    let start = content.find('😀').expect("the emoji is there");
    // Inside the four-byte emoji: not a character boundary.
    let edits = vec![LineEdit::new(1, 1, start, start + 2, "x")];
    let error = apply_edits(content, &edits).expect_err("a split emoji is refused");
    assert!(matches!(error, DomainError::Invariant(_)), "{error:?}");
    assert!(error.to_string().contains("character boundary"), "{error}");
}

#[test]
fn a_proposal_reproduces_its_new_content() {
    let content = "const decoded = jwt.decode(token);\n";
    let proposal = PatchProposal::new(
        "src/a.ts",
        "SEC-JWT-003",
        "abc123",
        1,
        vec![LineEdit::new(
            1,
            1,
            "const decoded = ".len(),
            "const decoded = jwt.decode(token)".len(),
            "jwt.verify(token)",
        )],
        "const decoded = jwt.verify(token);\n",
        "verify the token",
    );
    assert!(proposal.has_valid_edits());
    assert_eq!(
        proposal.apply_edits(content).expect("applies"),
        proposal.new_content
    );
}

#[test]
fn the_outcome_reports_its_status() {
    let applied = PatchOutcome::Applied {
        file_path: "src/a.ts".to_string(),
        original_sha256: "aaa".to_string(),
        new_sha256: "bbb".to_string(),
    };
    assert_eq!(applied.status_token(), "applied");
    assert!(applied.is_applied());
    assert_eq!(applied.file_path(), "src/a.ts");

    let conflict = PatchOutcome::Conflict {
        file_path: "src/a.ts".to_string(),
        expected_sha256: "aaa".to_string(),
        actual_sha256: "ccc".to_string(),
    };
    assert_eq!(conflict.status_token(), "conflict");
    assert!(!conflict.is_applied());

    let rejected = PatchOutcome::Rejected {
        file_path: "src/a.ts".to_string(),
        reason: "outside the workspace".to_string(),
    };
    assert_eq!(rejected.status_token(), "rejected");
    assert!(!rejected.is_applied());
}

#[test]
fn the_compliance_score_counts_the_clean_files() {
    let mut session =
        ScanSession::new("scan-1", "/repo", ScanMode::FullScan, 1_000).with_files_scanned(4);
    session.push(Finding::new("SEC-INJ-001", "a.ts", 1, 1).with_severity(Severity::Critical));
    session.push(Finding::new("SEC-XSS-004", "a.ts", 9, 9).with_severity(Severity::High));

    // Only one of the four files carries a finding: 75% of the files pass.
    assert_eq!(session.compliance_percent(), 75);

    let summary = session.summary();
    assert_eq!(summary.files_scanned, 4);
    assert_eq!(summary.compliance_percent, 75);
    assert_eq!(summary.total_findings, 2);
}

#[test]
fn a_scan_without_files_is_compliant() {
    let session = ScanSession::new("scan-1", "/repo", ScanMode::FullScan, 1_000);
    assert_eq!(session.compliance_percent(), 100);
    assert_eq!(session.summary().compliance_percent, 100);
}
