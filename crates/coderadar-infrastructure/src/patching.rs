//! Safe, atomic application of a [`PatchProposal`].
//!
//! Two independent safety mechanisms protect the user workspace
//! (`CR-SPEC-13 §4`):
//!
//! * **optimistic concurrency** — the fingerprint of the file on disk is
//!   compared with the fingerprint recorded when the diff was produced; a
//!   mismatch is reported as [`PatchOutcome::Conflict`] and nothing is
//!   written;
//! * **payload verification** — the advertised edits are re-applied to the
//!   current content and the result must match the advertised new content, so
//!   a tampered payload is rejected before it can corrupt a file;
//! * **atomic writes** — the new content lands in a temporary file in the
//!   same directory and is then `rename`d over the target, so the file is
//!   never observed half-written.
//!
//! Every refusal is reported as a [`PatchOutcome`], never as an error: a
//! refusal is a normal, expected outcome of the pipeline.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use coderadar_domain::{DomainError, PatchOutcome, PatchProposal};
use coderadar_usecases::PatchApplier;

use crate::hashing::sha256_hex;
use crate::workspace_guard::WorkspaceGuard;

/// Counter making the temporary file name unique inside one process.
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Reason reported when the payload of a proposal does not match the file.
const PAYLOAD_MISMATCH: &str = "the proposal payload does not match the current file content";

/// Build the path of the temporary file used to stage a write.
///
/// The temporary file lives in the **same directory** as the target so that the
/// final `rename` is atomic (same filesystem) and so that the target's
/// permissions can be inherited from the original file.
fn staging_path(path: &Path) -> PathBuf {
    let parent = path.parent().filter(|p| !p.as_os_str().is_empty());
    let directory = parent.unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".to_string());
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    directory.join(format!(".{name}.coderadar-{pid}-{counter}.tmp"))
}

/// Write `content` to `path` atomically.
///
/// The content is written to a uniquely named temporary file in the same
/// directory, flushed to the disk, given the permissions of the original file
/// and finally `rename`d over the target. A reader either sees the old
/// content or the new one, never a partially written file.
///
/// # Errors
/// Returns [`DomainError::Io`] when the staging file cannot be written or the
/// final rename fails. The staging file is removed in that case.
pub fn atomic_write(path: &Path, content: &str) -> Result<(), DomainError> {
    let staging = staging_path(path);
    match write_and_rename(path, content, &staging) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = std::fs::remove_file(&staging);
            Err(error)
        }
    }
}

/// Write the staging file and rename it over the target.
fn write_and_rename(path: &Path, content: &str, staging: &Path) -> Result<(), DomainError> {
    {
        let mut file = std::fs::File::create(staging).map_err(|error| {
            DomainError::Io(format!(
                "cannot create the staging file `{}`: {error}",
                staging.display()
            ))
        })?;
        file.write_all(content.as_bytes())
            .map_err(|error| DomainError::Io(format!("cannot write the content: {error}")))?;
        file.flush()
            .map_err(|error| DomainError::Io(format!("cannot flush the content: {error}")))?;
        file.sync_all().map_err(|error| {
            DomainError::Io(format!("cannot sync the content to the disk: {error}"))
        })?;

        // The target keeps its permissions across the rewrite.
        if let Ok(metadata) = std::fs::metadata(path) {
            let _ = file.set_permissions(metadata.permissions());
        }
    }

    std::fs::rename(staging, path).map_err(|error| {
        DomainError::Io(format!(
            "cannot replace `{}` with the staged content: {error}",
            path.display()
        ))
    })?;

    // Best effort: persist the directory entry itself (unix only; Windows
    // offers no equivalent through the standard library).
    #[cfg(unix)]
    if let Some(parent) = path.parent() {
        if let Ok(directory) = std::fs::File::open(parent) {
            let _ = directory.sync_all();
        }
    }

    Ok(())
}

/// Applies a [`PatchProposal`] to the workspace, safely.
#[derive(Debug, Clone, Copy, Default)]
pub struct SafePatchApplier;

impl SafePatchApplier {
    /// Create an applier.
    pub fn new() -> Self {
        Self
    }
}

impl PatchApplier for SafePatchApplier {
    fn apply_patch<'a>(
        &'a self,
        root: &'a str,
        proposal: &'a PatchProposal,
    ) -> coderadar_usecases::BoxFuture<'a, Result<PatchOutcome, DomainError>> {
        Box::pin(async move {
            let file_path = proposal.file_path.clone();

            // 1. Containment: the target must live inside the workspace.
            let guard = match WorkspaceGuard::new(root) {
                Ok(guard) => guard,
                Err(error) => {
                    return Ok(PatchOutcome::Rejected {
                        file_path,
                        reason: format!("the workspace root cannot be used: {error}"),
                    })
                }
            };
            let resolved = match guard.resolve(&file_path) {
                Ok(resolved) => resolved,
                Err(error) => {
                    return Ok(PatchOutcome::Rejected {
                        file_path,
                        reason: error.to_string(),
                    })
                }
            };

            // 2. The current content of the target.
            let current = match std::fs::read_to_string(&resolved) {
                Ok(current) => current,
                Err(error) => {
                    return Ok(PatchOutcome::Rejected {
                        file_path,
                        reason: format!("the target file cannot be read: {error}"),
                    })
                }
            };

            // 3. Optimistic concurrency: the file must still be the one the
            //    proposal was built against.
            let actual_sha256 = sha256_hex(&current);
            if actual_sha256 != proposal.original_sha256 {
                return Ok(PatchOutcome::Conflict {
                    file_path,
                    expected_sha256: proposal.original_sha256.clone(),
                    actual_sha256,
                });
            }

            // 4. The payload must be the honest result of the advertised
            //    edits applied to the current content.
            let reapplied = match proposal.apply_edits(&current) {
                Ok(reapplied) => reapplied,
                Err(_) => {
                    return Ok(PatchOutcome::Rejected {
                        file_path,
                        reason: PAYLOAD_MISMATCH.to_string(),
                    })
                }
            };
            if reapplied != proposal.new_content {
                return Ok(PatchOutcome::Rejected {
                    file_path,
                    reason: PAYLOAD_MISMATCH.to_string(),
                });
            }

            // 5. Atomic write.
            atomic_write(&resolved, &proposal.new_content)?;

            Ok(PatchOutcome::Applied {
                file_path,
                original_sha256: proposal.original_sha256.clone(),
                new_sha256: sha256_hex(&proposal.new_content),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{atomic_write, SafePatchApplier};
    use coderadar_domain::{DomainError, LineEdit, PatchOutcome, PatchProposal};
    use coderadar_usecases::PatchApplier;

    fn proposal_for(original: &str, new_content: &str, replacement: &str) -> PatchProposal {
        let original_sha256 = crate::hashing::sha256_hex(original);
        let edits = vec![LineEdit::new(1, 1, 0, original.len(), replacement)];
        PatchProposal::new(
            "a.txt",
            "SEC-001",
            original_sha256,
            original.lines().count(),
            edits,
            new_content,
            "test patch",
        )
    }

    fn apply(root: &std::path::Path, proposal: &PatchProposal) -> PatchOutcome {
        let root = root.to_str().expect("utf-8 path").to_string();
        let applier = SafePatchApplier::new();
        crate::executor::block_on(applier.apply_patch(&root, proposal)).expect("no error")
    }

    #[test]
    fn a_valid_patch_is_applied() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("a.txt"), "old").expect("write");

        let proposal = proposal_for("old", "new", "new");
        let outcome = apply(dir.path(), &proposal);
        assert_eq!(
            outcome,
            PatchOutcome::Applied {
                file_path: "a.txt".to_string(),
                original_sha256: crate::hashing::sha256_hex("old"),
                new_sha256: crate::hashing::sha256_hex("new"),
            }
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("a.txt")).expect("read"),
            "new"
        );
    }

    #[test]
    fn a_stale_proposal_is_a_conflict_and_the_file_is_untouched() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("a.txt"), "changed by the user").expect("write");

        let proposal = proposal_for("old", "new", "new");
        let outcome = apply(dir.path(), &proposal);
        assert!(
            matches!(outcome, PatchOutcome::Conflict { .. }),
            "{outcome:?}"
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("a.txt")).expect("read"),
            "changed by the user"
        );
    }

    #[test]
    fn a_tampered_payload_is_rejected_and_the_file_is_untouched() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("a.txt"), "old").expect("write");

        let mut proposal = proposal_for("old", "new", "new");
        proposal.new_content = "pwned".to_string();
        let outcome = apply(dir.path(), &proposal);
        match outcome {
            PatchOutcome::Rejected { reason, .. } => {
                assert!(
                    reason.contains("does not match the current file content"),
                    "{reason}"
                );
            }
            other => panic!("unexpected outcome: {other:?}"),
        }
        assert_eq!(
            std::fs::read_to_string(dir.path().join("a.txt")).expect("read"),
            "old"
        );
    }

    #[test]
    fn a_traversal_is_rejected() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut proposal = proposal_for("old", "new", "new");
        proposal.file_path = "../escape.txt".to_string();
        let outcome = apply(dir.path(), &proposal);
        assert!(
            matches!(outcome, PatchOutcome::Rejected { .. }),
            "{outcome:?}"
        );
        assert!(!dir
            .path()
            .parent()
            .expect("parent")
            .join("escape.txt")
            .exists());
    }

    #[test]
    fn atomic_write_leaves_no_staging_file_behind() {
        let dir = tempfile::tempdir().expect("tempdir");
        let target = dir.path().join("a.txt");
        std::fs::write(&target, "old").expect("write");
        atomic_write(&target, "new").expect("write");
        assert_eq!(std::fs::read_to_string(&target).expect("read"), "new");
        let entries: Vec<String> = std::fs::read_dir(dir.path())
            .expect("read_dir")
            .map(|entry| {
                entry
                    .expect("entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        assert_eq!(entries, vec!["a.txt".to_string()]);
    }

    #[test]
    fn atomic_write_keeps_the_permissions_of_the_target() {
        let dir = tempfile::tempdir().expect("tempdir");
        let target = dir.path().join("a.txt");
        std::fs::write(&target, "old").expect("write");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600))
                .expect("chmod");
            atomic_write(&target, "new").expect("write");
            let mode = std::fs::metadata(&target)
                .expect("metadata")
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600, "the mode must be preserved");
        }
        #[cfg(not(unix))]
        {
            atomic_write(&target, "new").expect("write");
            assert_eq!(std::fs::read_to_string(&target).expect("read"), "new");
        }
    }

    #[test]
    fn atomic_write_on_a_missing_file_reports_an_io_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let error =
            atomic_write(&dir.path().join("missing").join("a.txt"), "x").expect_err("must fail");
        assert!(matches!(error, DomainError::Io(_)), "{error:?}");
    }
}
