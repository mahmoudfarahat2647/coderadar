//! Patch-safety gate tests.
//!
//! These tests prove the core safety promise of the product: a workspace file
//! is never corrupted, never silently overwritten, and never written outside
//! the workspace (`CR-SPEC-13 §4`).

use std::path::Path;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

use coderadar_domain::{LineEdit, PatchOutcome, PatchProposal};
use coderadar_infrastructure::{sha256_hex, SafePatchApplier};
use coderadar_usecases::PatchApplier;

/// Block the current thread until `future` completes.
fn block_on<F: std::future::Future>(future: F) -> F::Output {
    struct ThreadWaker {
        thread: std::thread::Thread,
    }

    impl Wake for ThreadWaker {
        fn wake(self: Arc<Self>) {
            self.thread.unpark();
        }

        fn wake_by_ref(self: &Arc<Self>) {
            self.thread.unpark();
        }
    }

    let waker = Waker::from(Arc::new(ThreadWaker {
        thread: std::thread::current(),
    }));
    let mut context = Context::from_waker(&waker);
    let mut future = Box::pin(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => return output,
            Poll::Pending => std::thread::park(),
        }
    }
}

/// Build a proposal replacing the whole content of `file_path`.
fn proposal(file_path: &str, original: &str, new_content: &str) -> PatchProposal {
    let edits = vec![LineEdit::new(1, 1, 0, original.len(), new_content)];
    PatchProposal::new(
        file_path,
        "SEC-001",
        sha256_hex(original),
        original.lines().count(),
        edits,
        new_content,
        "replaces the whole file",
    )
}

fn apply(root: &Path, proposal: &PatchProposal) -> PatchOutcome {
    let root = root.to_str().expect("utf-8 path").to_string();
    let applier = SafePatchApplier::new();
    block_on(applier.apply_patch(&root, proposal)).expect("the applier never fails with an error")
}

/// List the entries of `directory`.
fn entries(directory: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(directory)
        .expect("read_dir")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

#[test]
fn a_valid_patch_rewrites_the_file_and_leaves_no_staging_file() {
    let directory = tempfile::tempdir().expect("tempdir");
    std::fs::write(directory.path().join("app.ts"), "const secret = 'old';\n").expect("write");

    let proposal = proposal(
        "app.ts",
        "const secret = 'old';\n",
        "const secret = process.env.SECRET;\n",
    );
    let outcome = apply(directory.path(), &proposal);

    match &outcome {
        PatchOutcome::Applied {
            file_path,
            original_sha256,
            new_sha256,
        } => {
            assert_eq!(file_path, "app.ts");
            assert_eq!(original_sha256, &sha256_hex("const secret = 'old';\n"));
            assert_eq!(
                new_sha256,
                &sha256_hex("const secret = process.env.SECRET;\n")
            );
        }
        other => panic!("unexpected outcome: {other:?}"),
    }

    assert_eq!(
        std::fs::read_to_string(directory.path().join("app.ts")).expect("read"),
        "const secret = process.env.SECRET;\n"
    );
    assert_eq!(
        entries(directory.path()),
        vec!["app.ts".to_string()],
        "no staging file may be left behind"
    );
}

#[test]
fn a_stale_proposal_is_a_conflict_and_the_file_is_untouched() {
    let directory = tempfile::tempdir().expect("tempdir");
    let target = directory.path().join("app.ts");
    std::fs::write(&target, "const secret = 'edited by the user';\n").expect("write");

    // The proposal was produced against a different content.
    let proposal = proposal(
        "app.ts",
        "const secret = 'old';\n",
        "const secret = process.env.SECRET;\n",
    );
    let outcome = apply(directory.path(), &proposal);

    match &outcome {
        PatchOutcome::Conflict {
            file_path,
            expected_sha256,
            actual_sha256,
        } => {
            assert_eq!(file_path, "app.ts");
            assert_eq!(expected_sha256, &sha256_hex("const secret = 'old';\n"));
            assert_eq!(
                actual_sha256,
                &sha256_hex("const secret = 'edited by the user';\n")
            );
        }
        other => panic!("unexpected outcome: {other:?}"),
    }

    assert_eq!(
        std::fs::read_to_string(&target).expect("read"),
        "const secret = 'edited by the user';\n",
        "the file must keep the content it has on disk"
    );
    assert_eq!(entries(directory.path()), vec!["app.ts".to_string()]);
}

#[test]
fn a_traversal_attempt_is_rejected_and_nothing_is_written_outside() {
    let directory = tempfile::tempdir().expect("tempdir");
    let workspace = directory.path().join("workspace");
    std::fs::create_dir_all(&workspace).expect("mkdir");
    let outside = directory.path().join("evil.txt");

    let proposal = proposal("../../evil.txt", "old", "new");
    let outcome = apply(&workspace, &proposal);

    match &outcome {
        PatchOutcome::Rejected { file_path, reason } => {
            assert_eq!(file_path, "../../evil.txt");
            assert!(!reason.is_empty());
        }
        other => panic!("unexpected outcome: {other:?}"),
    }

    assert!(
        !outside.exists(),
        "no file may be created outside the workspace"
    );
    assert_eq!(entries(&workspace), Vec::<String>::new());
}

#[test]
fn an_absolute_path_is_rejected() {
    let directory = tempfile::tempdir().expect("tempdir");
    let absolute = directory.path().join("absolute.txt");
    let absolute = absolute.to_str().expect("utf-8 path").to_string();

    let proposal = proposal(&absolute, "old", "new");
    let outcome = apply(directory.path(), &proposal);

    assert!(
        matches!(outcome, PatchOutcome::Rejected { .. }),
        "unexpected outcome: {outcome:?}"
    );
    assert!(!Path::new(&absolute).exists(), "no file may be created");
    assert_eq!(entries(directory.path()), Vec::<String>::new());
}

#[test]
fn a_symlink_pointing_outside_the_workspace_is_rejected() {
    let outside = tempfile::tempdir().expect("tempdir");
    let secret = outside.path().join("secret.txt");
    std::fs::write(&secret, "top secret").expect("write");

    let workspace = tempfile::tempdir().expect("tempdir");

    #[cfg(unix)]
    let link_result = std::os::unix::fs::symlink(&secret, workspace.path().join("link.txt"));
    #[cfg(windows)]
    let link_result =
        std::os::windows::fs::symlink_file(&secret, workspace.path().join("link.txt"));

    if link_result.is_err() {
        // Creating a symlink on Windows requires the developer mode privilege
        // or an elevated shell: the guard itself is covered by the unit tests
        // of the `workspace_guard` module on unix.
        eprintln!("skipping the symlink case: the symlink could not be created");
        return;
    }

    let proposal = proposal("link.txt", "top secret", "pwned");
    let outcome = apply(workspace.path(), &proposal);

    assert!(
        matches!(outcome, PatchOutcome::Rejected { .. }),
        "unexpected outcome: {outcome:?}"
    );
    assert_eq!(
        std::fs::read_to_string(&secret).expect("read"),
        "top secret",
        "the file outside the workspace must not be touched"
    );
}

#[test]
fn a_tampered_payload_is_never_written() {
    let directory = tempfile::tempdir().expect("tempdir");
    let target = directory.path().join("app.ts");
    std::fs::write(&target, "original content\n").expect("write");

    let mut proposal = proposal("app.ts", "original content\n", "patched content\n");
    // The payload no longer matches the advertised edits.
    proposal.new_content = "pwned\n".to_string();

    let outcome = apply(directory.path(), &proposal);
    match &outcome {
        PatchOutcome::Rejected { reason, .. } => {
            assert!(
                reason.contains("does not match the current file content"),
                "{reason}"
            );
        }
        other => panic!("unexpected outcome: {other:?}"),
    }

    assert_eq!(
        std::fs::read_to_string(&target).expect("read"),
        "original content\n",
        "the file must not be modified"
    );
    assert_eq!(entries(directory.path()), vec!["app.ts".to_string()]);
}

#[test]
fn a_proposal_whose_edits_do_not_fit_the_file_is_never_written() {
    let directory = tempfile::tempdir().expect("tempdir");
    let target = directory.path().join("app.ts");
    std::fs::write(&target, "short\n").expect("write");

    // The edit addresses a range outside of the current content.
    let edits = vec![LineEdit::new(1, 1, 0, 5_000, "boom")];
    let proposal = PatchProposal::new(
        "app.ts",
        "SEC-001",
        sha256_hex("short\n"),
        1,
        edits,
        "boom",
        "out of range",
    );

    let outcome = apply(directory.path(), &proposal);
    assert!(
        matches!(outcome, PatchOutcome::Rejected { .. }),
        "unexpected outcome: {outcome:?}"
    );
    assert_eq!(
        std::fs::read_to_string(&target).expect("read"),
        "short\n",
        "the file must not be modified"
    );
}

#[test]
fn a_missing_target_file_is_rejected_rather_than_created() {
    let directory = tempfile::tempdir().expect("tempdir");

    let proposal = proposal("missing.ts", "old", "new");
    let outcome = apply(directory.path(), &proposal);

    assert!(
        matches!(outcome, PatchOutcome::Rejected { .. }),
        "unexpected outcome: {outcome:?}"
    );
    assert_eq!(entries(directory.path()), Vec::<String>::new());
}
