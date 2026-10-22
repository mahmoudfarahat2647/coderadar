//! Git changeset engine (`CR-SPEC-04 §8`, Phase 8).
//!
//! Reads the **staged** index of the repository at a workspace root and turns
//! it into the pure [`Changeset`] description the domain understands. The
//! library is `git2` (libgit2): the product never spawns a `git` process, which
//! the security guard (`NFR-05`) forbids.
//!
//! The engine counts line stats per file, collects removed lines that look like
//! exported / public signatures (`PR-BRK-002`), and exposes the changed paths
//! so a staged scan only analyses the files under review.

use std::path::Path;

use coderadar_domain::{is_public_api_removal, Changeset, DomainError, FileChange};
use coderadar_usecases::{BoxFuture, ChangesetGateway};
use git2::{DiffOptions, ErrorCode, Patch, Repository};

/// Reads staged changesets with libgit2.
#[derive(Debug, Clone, Copy, Default)]
pub struct GitChangesetReader;

impl GitChangesetReader {
    /// Create a reader.
    pub fn new() -> Self {
        Self
    }

    /// Read the staged changeset of the repository at `root`.
    ///
    /// # Errors
    /// Returns [`DomainError::NotFound`] when `root` is not a Git repository
    /// and [`DomainError::External`] when the index cannot be read.
    pub fn read_staged(root: &Path) -> Result<Changeset, DomainError> {
        let repository = Repository::open(root).map_err(|error| {
            if error.code() == ErrorCode::NotFound {
                DomainError::NotFound(format!(
                    "`{}` is not a Git repository (run the PR scan from its root)",
                    root.display()
                ))
            } else {
                DomainError::External(format!("cannot open the Git repository: {error}"))
            }
        })?;

        let index = repository.index().map_err(|error| {
            DomainError::External(format!("cannot read the Git index: {error}"))
        })?;

        let (base, base_tree) = match repository.head() {
            Ok(head) => {
                let tree = head
                    .peel_to_tree()
                    .map_err(|error| DomainError::External(format!("cannot read HEAD: {error}")))?;
                ("HEAD".to_string(), Some(tree))
            }
            Err(error) if matches!(error.code(), ErrorCode::UnbornBranch | ErrorCode::NotFound) => {
                // The repository has no commit yet: everything staged is new.
                ("empty".to_string(), None)
            }
            Err(error) => {
                return Err(DomainError::External(format!(
                    "cannot read the Git HEAD: {error}"
                )))
            }
        };

        let mut options = DiffOptions::new();
        options
            .include_typechange(true)
            .include_untracked(false)
            .recurse_untracked_dirs(false);

        let diff = repository
            .diff_tree_to_index(base_tree.as_ref(), Some(&index), Some(&mut options))
            .map_err(|error| {
                DomainError::External(format!("cannot diff HEAD against the index: {error}"))
            })?;

        let mut files = Vec::new();
        for (position, delta) in diff.deltas().enumerate() {
            let path = delta
                .new_file()
                .path()
                .or_else(|| delta.old_file().path())
                .map(slash_path)
                .unwrap_or_default();
            if path.is_empty() {
                continue;
            }

            let (additions, deletions, removed_public_api) = match Patch::from_diff(&diff, position)
            {
                Ok(Some(patch)) => summarise_patch(&patch)?,
                // A binary delta has no textual patch: it still counts as a
                // changed file, but it contributes no line stats.
                Ok(None) => (0, 0, Vec::new()),
                Err(error) => {
                    return Err(DomainError::External(format!(
                        "cannot read the patch of `{path}`: {error}"
                    )))
                }
            };

            files.push(
                FileChange::new(path, additions, deletions)
                    .with_removed_public_api(removed_public_api),
            );
        }

        Ok(Changeset::new(base, files))
    }
}

impl ChangesetGateway for GitChangesetReader {
    fn staged_changeset<'a>(
        &'a self,
        root: &'a str,
    ) -> BoxFuture<'a, Result<Changeset, DomainError>> {
        Box::pin(async move { Self::read_staged(Path::new(root)) })
    }
}

/// Line statistics plus removed public signatures of a textual patch.
fn summarise_patch(patch: &Patch<'_>) -> Result<(u32, u32, Vec<String>), DomainError> {
    let (_, additions, deletions) = patch.line_stats().map_err(|error| {
        DomainError::External(format!("cannot read the patch statistics: {error}"))
    })?;

    let mut removed_public_api = Vec::new();
    for hunk_index in 0..patch.num_hunks() {
        let lines = patch
            .num_lines_in_hunk(hunk_index)
            .map_err(|error| DomainError::External(format!("cannot read a hunk: {error}")))?;
        for line_index in 0..lines {
            let line = patch
                .line_in_hunk(hunk_index, line_index)
                .map_err(|error| DomainError::External(format!("cannot read a line: {error}")))?;
            if line.origin() != '-' {
                continue;
            }
            let content = std::str::from_utf8(line.content()).unwrap_or_default();
            let content = content.trim_end_matches(['\r', '\n']);
            if is_public_api_removal(content) {
                removed_public_api.push(content.to_string());
            }
        }
    }

    Ok((
        u32::try_from(additions).unwrap_or(u32::MAX),
        u32::try_from(deletions).unwrap_or(u32::MAX),
        removed_public_api,
    ))
}

/// Render a repository path with forward slashes.
fn slash_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use coderadar_domain::DomainError;
    use git2::{IndexAddOption, Repository, Signature};
    use tempfile::TempDir;

    use super::GitChangesetReader;

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

    fn stage(repository: &Repository, root: &Path, relative: &str) {
        let mut index = repository.index().expect("index");
        index
            .add_all([relative], IndexAddOption::DEFAULT, None)
            .expect("stage");
        index.write().expect("write index");
        let _ = root;
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

    #[test]
    fn reads_staged_file_statistics() {
        let directory = TempDir::new().unwrap();
        let root = directory.path();
        let repository = init_repository(root);
        write(
            root,
            "src/a.ts",
            "export const a = 1;\nconst internal = 2;\n",
        );
        stage(&repository, root, "src/a.ts");
        commit(&repository, "initial");

        write(
            root,
            "src/a.ts",
            "export const a = 1;\nexport const c = 3;\nexport const d = 4;\n",
        );
        stage(&repository, root, "src/a.ts");

        let changeset = GitChangesetReader::read_staged(root).expect("staged changeset");
        assert_eq!(changeset.base, "HEAD");
        assert_eq!(changeset.files_changed(), 1);
        assert_eq!(changeset.files[0].path, "src/a.ts");
        assert_eq!(changeset.files[0].additions, 2);
        assert_eq!(changeset.files[0].deletions, 1);
        assert!(changeset.files[0].removed_public_api.is_empty());
    }

    #[test]
    fn collects_removed_public_signatures() {
        let directory = TempDir::new().unwrap();
        let root = directory.path();
        let repository = init_repository(root);
        write(
            root,
            "src/api.ts",
            "export function oldApi() {}\nexport function kept() {}\n",
        );
        stage(&repository, root, "src/api.ts");
        commit(&repository, "initial");

        write(root, "src/api.ts", "export function kept() {}\n");
        stage(&repository, root, "src/api.ts");

        let changeset = GitChangesetReader::read_staged(root).expect("staged changeset");
        assert_eq!(
            changeset.files[0].removed_public_api,
            vec!["export function oldApi() {}".to_string()]
        );
        assert!(changeset.assess().breaking_change);
    }

    #[test]
    fn untracked_and_unstaged_files_are_ignored() {
        let directory = TempDir::new().unwrap();
        let root = directory.path();
        let repository = init_repository(root);
        write(root, "src/a.ts", "export const a = 1;\n");
        stage(&repository, root, "src/a.ts");
        commit(&repository, "initial");

        write(root, "src/unstaged.ts", "export const b = 2;\n");
        write(root, "src/untracked.ts", "export const c = 3;\n");
        stage(&repository, root, "src/untracked.ts");

        let changeset = GitChangesetReader::read_staged(root).expect("staged changeset");
        let paths: Vec<&str> = changeset
            .files
            .iter()
            .map(|file| file.path.as_str())
            .collect();
        assert_eq!(paths, vec!["src/untracked.ts"]);
    }

    #[test]
    fn a_repository_without_commits_compares_against_the_empty_tree() {
        let directory = TempDir::new().unwrap();
        let root = directory.path();
        let repository = init_repository(root);
        write(root, "src/a.ts", "export const a = 1;\n");
        stage(&repository, root, "src/a.ts");

        let changeset = GitChangesetReader::read_staged(root).expect("staged changeset");
        assert_eq!(changeset.base, "empty");
        assert_eq!(changeset.files_changed(), 1);
        assert_eq!(changeset.files[0].additions, 1);
    }

    #[test]
    fn a_directory_that_is_not_a_repository_is_reported() {
        let directory = TempDir::new().unwrap();
        let error = GitChangesetReader::read_staged(directory.path())
            .expect_err("a plain directory is not a repository");
        assert!(matches!(error, DomainError::NotFound(_)), "{error:?}");
    }
}
