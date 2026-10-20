//! Workspace containment guard — the security boundary of every file write.
//!
//! CodeRadar Studio edits user workspaces on behalf of AI-generated fixes.
//! A single unchecked path (`../../.ssh/authorized_keys`) would turn the
//! product into a remote-code-execution vector, so **every** read and write
//! goes through a [`WorkspaceGuard`] first (`CR-SPEC-13 §4`).
//!
//! The guard rejects, in this order:
//!
//! 1. empty paths;
//! 2. absolute paths;
//! 3. any path containing a `..` component (also after normalising the
//!    Windows `\` separator, so a Windows-style payload is rejected on every
//!    platform);
//! 4. any path whose *canonical* parent directory falls outside the canonical
//!    workspace root — this is the symlink-escape guard: a directory inside the
//!    workspace that links to `/etc` is refused;
//! 5. any path whose *canonical* target falls outside the canonical
//!    workspace root — a file inside the workspace that links to a secret is
//!    refused.
//!
//! All violations are reported as [`DomainError::Invariant`]; a missing
//! workspace root is reported as [`DomainError::NotFound`].

use std::path::{Component, Path, PathBuf};

use coderadar_domain::DomainError;

/// The canonical, slash separated path of a workspace.
///
/// The same spelling is stored with a scan session and used to scope the scan
/// history, so a stored session and a query always agree.
pub fn canonical_root(root: &Path) -> String {
    let canonical = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    canonical.to_string_lossy().replace('\\', "/")
}

/// A canonicalised workspace root that can resolve relative paths safely.
#[derive(Debug, Clone)]
pub struct WorkspaceGuard {
    root: PathBuf,
}

impl WorkspaceGuard {
    /// Create a guard for `root`.
    ///
    /// The root is canonicalised immediately, so every later comparison is
    /// made between two canonical paths.
    ///
    /// # Errors
    /// Returns [`DomainError::NotFound`] when the root does not exist and
    /// [`DomainError::Invariant`] when it exists but is not a directory.
    pub fn new(root: &str) -> Result<Self, DomainError> {
        let canonical = std::fs::canonicalize(root).map_err(|error| {
            DomainError::NotFound(format!(
                "workspace root `{root}` is not accessible: {error}"
            ))
        })?;
        if !canonical.is_dir() {
            return Err(DomainError::Invariant(format!(
                "workspace root `{root}` is not a directory"
            )));
        }
        Ok(Self { root: canonical })
    }

    /// The canonical workspace root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Resolve `relative_path` to an absolute path inside the workspace.
    ///
    /// The returned path is canonical when the target already exists, and the
    /// canonical parent joined with the file name otherwise (the target may
    /// not exist yet — that is the normal case for a new file).
    ///
    /// # Errors
    /// Returns [`DomainError::Invariant`] when the path is empty, absolute,
    /// contains a `..` component, or escapes the workspace root through a
    /// symlink.
    pub fn resolve(&self, relative_path: &str) -> Result<PathBuf, DomainError> {
        if relative_path.trim().is_empty() {
            return Err(DomainError::Invariant(
                "the path must not be empty".to_string(),
            ));
        }

        let candidate = Path::new(relative_path);
        if candidate.is_absolute() {
            return Err(DomainError::Invariant(format!(
                "absolute paths are not allowed: `{relative_path}`"
            )));
        }
        if candidate
            .components()
            .any(|component| matches!(component, Component::Prefix(_) | Component::RootDir))
        {
            return Err(DomainError::Invariant(format!(
                "absolute paths are not allowed: `{relative_path}`"
            )));
        }
        if contains_parent_component(relative_path) {
            return Err(DomainError::Invariant(format!(
                "path escapes the workspace root: `{relative_path}`"
            )));
        }

        let joined = self.root.join(candidate);

        // Symlink-escape guard on the parent directory: it must exist and it
        // must still live inside the workspace once every symlink is resolved.
        let parent = joined.parent().ok_or_else(|| {
            DomainError::Invariant(format!(
                "path escapes the workspace root: `{relative_path}`"
            ))
        })?;
        let canonical_parent = std::fs::canonicalize(parent).map_err(|error| {
            DomainError::Invariant(format!(
                "cannot resolve the parent directory of `{relative_path}`: {error}"
            ))
        })?;
        if !canonical_parent.starts_with(&self.root) {
            return Err(DomainError::Invariant(format!(
                "path escapes the workspace root: `{relative_path}`"
            )));
        }

        let file_name = joined.file_name().ok_or_else(|| {
            DomainError::Invariant(format!(
                "path escapes the workspace root: `{relative_path}`"
            ))
        })?;

        // Symlink-escape guard on the target itself. `symlink_metadata` does
        // not follow the link, so a dangling link is detected as well.
        let target = canonical_parent.join(file_name);
        if std::fs::symlink_metadata(&target).is_ok() {
            let canonical_target = std::fs::canonicalize(&target).map_err(|error| {
                DomainError::Invariant(format!(
                    "cannot resolve `{relative_path}` inside the workspace: {error}"
                ))
            })?;
            if !canonical_target.starts_with(&self.root) {
                return Err(DomainError::Invariant(format!(
                    "path escapes the workspace root: `{relative_path}`"
                )));
            }
            return Ok(canonical_target);
        }

        Ok(target)
    }
}

/// `true` when `path` contains a `..` component once the Windows separator is
/// normalised to `/`.
fn contains_parent_component(path: &str) -> bool {
    let normalised = path.replace('\\', "/");
    normalised.split('/').any(|segment| segment == "..")
}

#[cfg(test)]
mod tests {
    use super::WorkspaceGuard;
    use coderadar_domain::DomainError;

    fn guard(root: &std::path::Path) -> WorkspaceGuard {
        WorkspaceGuard::new(root.to_str().expect("utf-8 path")).expect("guard")
    }

    #[test]
    fn missing_root_is_reported_as_not_found() {
        let missing = std::path::Path::new("this-directory-does-not-exist-coderadar");
        let error = WorkspaceGuard::new(missing.to_str().expect("utf-8 path"))
            .expect_err("missing root must fail");
        assert!(matches!(error, DomainError::NotFound(_)), "{error:?}");
    }

    #[test]
    fn a_file_is_not_a_valid_root() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("file.txt");
        std::fs::write(&file, "x").expect("write");
        let error = WorkspaceGuard::new(file.to_str().expect("utf-8 path")).expect_err("must fail");
        assert!(matches!(error, DomainError::Invariant(_)), "{error:?}");
    }

    #[test]
    fn a_normal_relative_path_resolves_inside_the_root() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("src")).expect("mkdir");
        std::fs::write(dir.path().join("src").join("main.rs"), "fn main() {}").expect("write");

        let guard = guard(dir.path());
        let resolved = guard.resolve("src/main.rs").expect("resolve");
        assert!(resolved.starts_with(guard.root()));
        assert!(resolved.ends_with("main.rs"));
    }

    #[test]
    fn a_missing_target_resolves_to_the_canonical_parent() {
        let dir = tempfile::tempdir().expect("tempdir");
        let guard = guard(dir.path());
        let resolved = guard.resolve("new-file.rs").expect("resolve");
        assert_eq!(resolved, guard.root().join("new-file.rs"));
    }

    #[test]
    fn a_parent_traversal_is_rejected() {
        let dir = tempfile::tempdir().expect("tempdir");
        let guard = guard(dir.path());
        let error = guard.resolve("../outside.txt").expect_err("must fail");
        assert!(matches!(error, DomainError::Invariant(_)), "{error:?}");
        assert!(error.to_string().contains("escapes the workspace root"));
    }

    #[test]
    fn a_nested_parent_traversal_is_rejected() {
        let dir = tempfile::tempdir().expect("tempdir");
        let guard = guard(dir.path());
        for path in ["a/../../b.txt", "..", "a/b/../../../c.txt"] {
            let error = guard.resolve(path).expect_err("must fail");
            assert!(
                matches!(error, DomainError::Invariant(_)),
                "{path}: {error:?}"
            );
        }
    }

    #[test]
    fn a_windows_style_traversal_is_rejected() {
        let dir = tempfile::tempdir().expect("tempdir");
        let guard = guard(dir.path());
        let error = guard.resolve("..\\outside.txt").expect_err("must fail");
        assert!(matches!(error, DomainError::Invariant(_)), "{error:?}");
    }

    #[test]
    fn an_absolute_path_is_rejected() {
        let dir = tempfile::tempdir().expect("tempdir");
        let guard = guard(dir.path());
        let absolute = if cfg!(windows) {
            "C:\\Windows\\System32\\config\\SAM"
        } else {
            "/etc/passwd"
        };
        let error = guard.resolve(absolute).expect_err("must fail");
        assert!(matches!(error, DomainError::Invariant(_)), "{error:?}");
    }

    #[test]
    fn an_empty_path_is_rejected() {
        let dir = tempfile::tempdir().expect("tempdir");
        let guard = guard(dir.path());
        for path in ["", "   "] {
            let error = guard.resolve(path).expect_err("must fail");
            assert!(
                matches!(error, DomainError::Invariant(_)),
                "{path:?}: {error:?}"
            );
        }
    }

    #[test]
    #[cfg(unix)]
    fn a_symlink_pointing_outside_the_workspace_is_rejected() {
        use std::os::unix::fs::symlink;

        let outside = tempfile::tempdir().expect("tempdir");
        let secret = outside.path().join("secret.txt");
        std::fs::write(&secret, "secret").expect("write");

        let workspace = tempfile::tempdir().expect("tempdir");
        let guard = guard(workspace.path());

        // A symlinked *file* pointing outside the workspace.
        symlink(&secret, workspace.path().join("link.txt")).expect("symlink");
        let error = guard.resolve("link.txt").expect_err("must fail");
        assert!(matches!(error, DomainError::Invariant(_)), "{error:?}");
        assert!(error.to_string().contains("escapes the workspace root"));

        // A symlinked *directory* pointing outside the workspace.
        symlink(outside.path(), workspace.path().join("linkdir")).expect("symlink");
        let error = guard.resolve("linkdir/secret.txt").expect_err("must fail");
        assert!(matches!(error, DomainError::Invariant(_)), "{error:?}");
    }
}
