//! Patch model — the pure, filesystem-free description of a proposed change.
//!
//! A [`PatchProposal`] is produced from a finding plus the current content of
//! the target file, and is applied later by an outer layer that owns the
//! filesystem. Everything in this module is pure data plus pure functions:
//! the domain never learns how a file is read, hashed, or written.

use crate::error::DomainError;

/// A replacement of a byte range inside a file.
///
/// The offsets address the *original* content, so a set of edits can be
/// applied in one deterministic pass. The line numbers are carried alongside
/// for display (the diff modal highlights the affected lines) and are derived
/// from the offsets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineEdit {
    /// First affected line, 1-based and inclusive.
    pub start_line: u32,
    /// Last affected line, 1-based and inclusive.
    pub end_line: u32,
    /// Byte offset (inclusive) of the replaced range in the original content.
    pub start_offset: usize,
    /// Byte offset (exclusive) of the replaced range in the original content.
    pub end_offset: usize,
    /// The replacement text.
    pub replacement: String,
}

impl LineEdit {
    /// Create a byte-range replacement.
    pub fn new(
        start_line: u32,
        end_line: u32,
        start_offset: usize,
        end_offset: usize,
        replacement: impl Into<String>,
    ) -> Self {
        Self {
            start_line,
            end_line,
            start_offset,
            end_offset,
            replacement: replacement.into(),
        }
    }

    /// `true` when the edit addresses a valid, non-empty range.
    pub fn is_valid(&self) -> bool {
        self.start_line >= 1
            && self.end_line >= self.start_line
            && self.end_offset >= self.start_offset
    }
}

/// A fully prepared, reviewable change to a single workspace file.
///
/// The proposal carries the SHA-256 fingerprint of the file as it was when
/// the diff was produced. An applier must refuse the proposal when the file
/// on disk no longer matches that fingerprint (optimistic concurrency).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchProposal {
    /// Workspace-relative path, slash separated.
    pub file_path: String,
    /// The rule that motivated the change.
    pub rule_id: String,
    /// SHA-256 (hex) of the original file content.
    pub original_sha256: String,
    /// Number of lines in the original file.
    pub original_line_count: usize,
    /// The line edits that produce the new content.
    pub edits: Vec<LineEdit>,
    /// The resulting file content.
    pub new_content: String,
    /// Human readable justification shown in the diff modal.
    pub explanation: String,
}

impl PatchProposal {
    /// Create a proposal.
    pub fn new(
        file_path: impl Into<String>,
        rule_id: impl Into<String>,
        original_sha256: impl Into<String>,
        original_line_count: usize,
        edits: Vec<LineEdit>,
        new_content: impl Into<String>,
        explanation: impl Into<String>,
    ) -> Self {
        Self {
            file_path: file_path.into(),
            rule_id: rule_id.into(),
            original_sha256: original_sha256.into(),
            original_line_count,
            edits,
            new_content: new_content.into(),
            explanation: explanation.into(),
        }
    }

    /// `true` when every edit addresses a valid line range.
    pub fn has_valid_edits(&self) -> bool {
        self.edits.iter().all(LineEdit::is_valid)
    }

    /// Re-apply the edits to `content` and return the resulting text.
    ///
    /// This is the deterministic part of the safety model: an applier uses it
    /// to prove that the payload it received really is the result of the
    /// advertised edits, applied to the file that is on disk right now.
    ///
    /// # Errors
    /// Returns [`DomainError::Invariant`] when an edit addresses an invalid
    /// line range or a range outside `content`.
    pub fn apply_edits(&self, content: &str) -> Result<String, DomainError> {
        apply_edits(content, &self.edits)
    }
}

/// Apply a set of line edits to `content`.
///
/// The edits address byte ranges of the *original* content and are applied
/// from the end of the file towards its beginning, so an earlier edit never
/// invalidates the offsets of a later one. Overlapping ranges are rejected.
///
/// # Errors
/// Returns [`DomainError::Invariant`] when a range is invalid, out of bounds,
/// or overlaps another range.
pub fn apply_edits(content: &str, edits: &[LineEdit]) -> Result<String, DomainError> {
    let mut ordered: Vec<&LineEdit> = edits.iter().collect();
    ordered.sort_by_key(|edit| std::cmp::Reverse(edit.start_offset));

    let mut previous_start = usize::MAX;
    for edit in &ordered {
        if !edit.is_valid() {
            return Err(DomainError::invariant(format!(
                "invalid edit range {}..={}",
                edit.start_offset, edit.end_offset
            )));
        }
        if edit.end_offset > content.len() {
            return Err(DomainError::invariant(format!(
                "edit range {}..={} exceeds content length {}",
                edit.start_offset,
                edit.end_offset,
                content.len()
            )));
        }
        if edit.end_offset > previous_start {
            return Err(DomainError::invariant(
                "overlapping edits are not allowed".to_string(),
            ));
        }
        previous_start = edit.start_offset;
    }

    let mut out = content.to_string();
    for edit in ordered {
        out.replace_range(edit.start_offset..edit.end_offset, &edit.replacement);
    }
    Ok(out)
}

/// The result of applying a [`PatchProposal`] to the workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatchOutcome {
    /// The file was rewritten successfully.
    Applied {
        /// Workspace-relative path of the patched file.
        file_path: String,
        /// SHA-256 of the file before the patch.
        original_sha256: String,
        /// SHA-256 of the file after the patch.
        new_sha256: String,
    },
    /// The file changed on disk since the diff was produced; nothing was written.
    Conflict {
        /// Workspace-relative path of the conflicting file.
        file_path: String,
        /// The fingerprint the proposal was built against.
        expected_sha256: String,
        /// The fingerprint of the file on disk right now.
        actual_sha256: String,
    },
    /// A guard refused the patch; nothing was written.
    Rejected {
        /// Workspace-relative path of the refused file.
        file_path: String,
        /// Why the patch was refused.
        reason: String,
    },
}

impl PatchOutcome {
    /// Stable status token used across boundaries.
    pub fn status_token(&self) -> &'static str {
        match self {
            Self::Applied { .. } => "applied",
            Self::Conflict { .. } => "conflict",
            Self::Rejected { .. } => "rejected",
        }
    }

    /// `true` when the file was actually rewritten.
    pub fn is_applied(&self) -> bool {
        matches!(self, Self::Applied { .. })
    }

    /// The workspace-relative path the outcome refers to.
    pub fn file_path(&self) -> &str {
        match self {
            Self::Applied { file_path, .. }
            | Self::Conflict { file_path, .. }
            | Self::Rejected { file_path, .. } => file_path,
        }
    }
}
