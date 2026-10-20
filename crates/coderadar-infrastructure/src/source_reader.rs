//! Filesystem implementation of the [`SourceFileReader`] port.
//!
//! The reader is the only path through which the use cases obtain file
//! content. It combines the [`WorkspaceGuard`] (containment) with a plain
//! UTF-8 read and produces the [`SourceFile`] fingerprint used by the
//! optimistic-concurrency model of the patch pipeline (`CR-SPEC-13 §4`).

use std::path::Path;

use coderadar_domain::DomainError;
use coderadar_usecases::{SourceFile, SourceFileReader};

use crate::hashing::sha256_hex;
use crate::workspace_guard::WorkspaceGuard;

/// Reads workspace files from the local filesystem.
#[derive(Debug, Clone, Copy, Default)]
pub struct FileSystemSourceReader;

impl FileSystemSourceReader {
    /// Create a reader.
    pub fn new() -> Self {
        Self
    }
}

/// Count the lines of `content`.
///
/// A trailing newline does **not** introduce an extra empty line, and an empty
/// content has zero lines.
fn count_lines(content: &str) -> usize {
    if content.is_empty() {
        return 0;
    }
    let newlines = content.bytes().filter(|byte| *byte == b'\n').count();
    if content.ends_with('\n') {
        newlines
    } else {
        newlines + 1
    }
}

impl SourceFileReader for FileSystemSourceReader {
    fn read_source<'a>(
        &'a self,
        root: &'a str,
        relative_path: &'a str,
    ) -> coderadar_usecases::BoxFuture<'a, Result<SourceFile, DomainError>> {
        Box::pin(async move {
            let guard = WorkspaceGuard::new(root)?;
            let resolved = guard.resolve(relative_path)?;
            let content = read_to_string(&resolved)?;
            let sha256 = sha256_hex(&content);
            let line_count = count_lines(&content);
            Ok(SourceFile {
                content,
                sha256,
                line_count,
            })
        })
    }
}

/// Read `path` as UTF-8 text.
///
/// # Errors
/// Returns [`DomainError::NotFound`] when the file does not exist and
/// [`DomainError::Io`] when it cannot be read or is not valid UTF-8.
fn read_to_string(path: &Path) -> Result<String, DomainError> {
    let bytes = std::fs::read(path).map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => {
            DomainError::NotFound(format!("`{}` does not exist", path.display()))
        }
        _ => DomainError::Io(format!("cannot read `{}`: {error}", path.display())),
    })?;
    String::from_utf8(bytes).map_err(|error| {
        DomainError::Io(format!("`{}` is not valid UTF-8: {error}", path.display()))
    })
}

#[cfg(test)]
mod tests {
    use super::{count_lines, FileSystemSourceReader};
    use coderadar_usecases::SourceFileReader;

    #[test]
    fn line_count_ignores_the_trailing_newline() {
        assert_eq!(count_lines(""), 0);
        assert_eq!(count_lines("a"), 1);
        assert_eq!(count_lines("a\n"), 1);
        assert_eq!(count_lines("a\nb"), 2);
        assert_eq!(count_lines("a\nb\n"), 2);
        assert_eq!(count_lines("\n"), 1);
    }

    #[test]
    fn reads_a_file_and_fingerprints_it() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("a.txt"), "hello\nworld\n").expect("write");

        let reader = FileSystemSourceReader::new();
        let root = dir.path().to_str().expect("utf-8 path").to_string();
        let source = crate::executor::block_on(reader.read_source(&root, "a.txt")).expect("read");

        assert_eq!(source.content, "hello\nworld\n");
        assert_eq!(source.line_count, 2);
        assert_eq!(
            source.sha256,
            crate::hashing::sha256_hex("hello\nworld\n"),
            "the fingerprint must cover the exact bytes of the file"
        );
    }

    #[test]
    fn a_traversal_is_refused() {
        let dir = tempfile::tempdir().expect("tempdir");
        let reader = FileSystemSourceReader::new();
        let root = dir.path().to_str().expect("utf-8 path").to_string();
        let error = crate::executor::block_on(reader.read_source(&root, "../escape.txt"))
            .expect_err("must fail");
        assert!(
            error.to_string().contains("escapes the workspace root"),
            "{error}"
        );
    }

    #[test]
    fn a_missing_file_is_reported_as_not_found() {
        let dir = tempfile::tempdir().expect("tempdir");
        let reader = FileSystemSourceReader::new();
        let root = dir.path().to_str().expect("utf-8 path").to_string();
        let error = crate::executor::block_on(reader.read_source(&root, "nope.txt"))
            .expect_err("must fail");
        assert!(
            matches!(error, coderadar_domain::DomainError::NotFound(_)),
            "{error:?}"
        );
    }
}
