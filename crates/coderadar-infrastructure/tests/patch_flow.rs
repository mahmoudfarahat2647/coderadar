//! End-to-end patch flow: read -> template -> diff -> review -> apply.
//!
//! This is the Phase 6 gate in its realistic shape: the same components the
//! Tauri commands use are exercised against a real workspace on disk
//! (`CR-SPEC-13 §4`).

use std::sync::Arc;

use coderadar_adapters::unified_diff;
use coderadar_domain::PatchOutcome;
use coderadar_infrastructure::{sha256_hex, FileSystemSourceReader, SafePatchApplier};
use coderadar_usecases::{PatchApplier, PreparePatch, SourceFileReader};

mod common;

use common::block_on;

const JWT_FILE: &str = "src/bad/jwt.ts";

/// Prepare the deterministic fix of `rule_id` for `path` inside `root`.
fn prepare(root: &str, path: &str, rule_id: &str) -> (coderadar_domain::PatchProposal, String) {
    let reader = Arc::new(FileSystemSourceReader::new());
    let source = block_on(reader.read_source(root, path)).expect("the source file is readable");
    let prepare = PreparePatch::new(reader);
    let proposal = block_on(prepare.prepare_from_source(&source, path, rule_id))
        .expect("the template matches");
    let diff = unified_diff(&source.content, &proposal.new_content, path);
    (proposal, diff.text)
}

#[test]
fn the_deterministic_jwt_fix_is_previewed_and_applied() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(dir.path().join("src/bad")).expect("mkdir");
    let file = dir.path().join(JWT_FILE);
    std::fs::write(
        &file,
        "export function read(token: string) {\n  return jwt.decode(token);\n}\n",
    )
    .expect("write");

    let root = dir.path().to_str().expect("utf-8 path");
    let (proposal, diff) = prepare(root, JWT_FILE, "SEC-JWT-003");

    // The preview shows the reviewer exactly what will change.
    assert!(diff.contains("-  return jwt.decode(token);"), "{diff}");
    assert!(
        diff.contains("+  return jwt.verify(token, process.env.JWT_SECRET);"),
        "{diff}"
    );

    let applier = SafePatchApplier::new();
    let outcome = block_on(applier.apply_patch(root, &proposal)).expect("no error");
    assert!(outcome.is_applied(), "{outcome:?}");

    let patched = std::fs::read_to_string(&file).expect("read");
    assert!(
        patched.contains("jwt.verify(token, process.env.JWT_SECRET)"),
        "{patched}"
    );
    assert_eq!(sha256_hex(&patched), sha256_hex(&proposal.new_content));
}

#[test]
fn a_stale_preview_is_refused_after_the_file_changed() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(dir.path().join("src/bad")).expect("mkdir");
    let file = dir.path().join(JWT_FILE);
    std::fs::write(&file, "const decoded = jwt.decode(token);\n").expect("write");

    let root = dir.path().to_str().expect("utf-8 path");
    let (proposal, _diff) = prepare(root, JWT_FILE, "SEC-JWT-003");

    // The user edits the file while the diff is open.
    std::fs::write(&file, "const decoded = jwt.decode(token); // touched\n").expect("write");

    let applier = SafePatchApplier::new();
    let outcome = block_on(applier.apply_patch(root, &proposal)).expect("no error");
    assert!(
        matches!(outcome, PatchOutcome::Conflict { .. }),
        "{outcome:?}"
    );

    // The user's edit survived untouched.
    let current = std::fs::read_to_string(&file).expect("read");
    assert!(current.ends_with("// touched\n"), "{current}");
}

#[test]
fn a_rule_without_a_deterministic_template_is_refused() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("a.ts"), "const a = 1;\n").expect("write");

    let root = dir.path().to_str().expect("utf-8 path");
    let reader = Arc::new(FileSystemSourceReader::new());
    let source = block_on(reader.read_source(root, "a.ts")).expect("readable");
    let prepare = PreparePatch::new(reader);
    let error = block_on(prepare.prepare_from_source(&source, "a.ts", "ARCH-SRP-002"))
        .expect_err("there is no template for this rule");
    assert!(
        error.to_string().contains("no deterministic fix template"),
        "{error}"
    );
}

#[test]
fn the_applier_refuses_a_path_outside_the_workspace() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("a.ts"), "const a = 1;\n").expect("write");

    let root = dir.path().to_str().expect("utf-8 path");
    std::fs::write(
        dir.path().join("a.ts"),
        "const apiKey = \"sk-live-9a8b7c6d5e4f3a2b1c0d\";\n",
    )
    .expect("write");
    let (mut proposal, _diff) = prepare(root, "a.ts", "SEC-TOK-002");
    proposal.file_path = "../escaped.ts".to_string();

    let applier = SafePatchApplier::new();
    let outcome = block_on(applier.apply_patch(root, &proposal)).expect("no error");
    assert!(
        matches!(outcome, PatchOutcome::Rejected { .. }),
        "{outcome:?}"
    );
    assert!(!dir
        .path()
        .parent()
        .expect("parent")
        .join("escaped.ts")
        .exists());
}
