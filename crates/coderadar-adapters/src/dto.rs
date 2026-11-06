use coderadar_domain::{
    Finding, LineEdit, PatchOutcome, PatchProposal, ScanCoverage, ScanSessionSummary, ScanUsage,
};
use serde::{Deserialize, Serialize};

use crate::radar::PolarCoordinate;

/// Lightweight, boundary-safe view of a [`Finding`].
///
/// Inner entities are never serialized across a boundary (`CR-SPEC-02 §3`); the
/// webview receives this flat DTO instead.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FindingDTO {
    /// Stable finding identifier.
    pub id: String,
    /// Offending file path.
    pub file: String,
    /// First offending line.
    pub line_start: u32,
    /// Last offending line.
    pub line_end: u32,
    /// Rule that produced the finding.
    pub rule_id: String,
    /// Review pillar / category token.
    pub category: String,
    /// Severity token.
    pub severity: String,
    /// Associated CWE, when applicable.
    pub cwe: Option<String>,
    /// Short title.
    pub title: String,
    /// Detailed explanation of the finding.
    pub description: String,
    /// Engine that produced the finding (`tier0` | `tier1` | `mock`).
    pub engine: String,
    /// Evaluator confidence in `[0.0, 1.0]`, when recorded.
    pub confidence: Option<f32>,
    /// Model probability, when recorded.
    #[serde(default)]
    pub model_probability: Option<f32>,
    /// Uncalibrated heuristic weight, when recorded.
    #[serde(default)]
    pub heuristic_weight: Option<f32>,
    /// Polar radius for the radar.
    pub radar_r: f32,
    /// Polar angle (degrees) for the radar.
    pub radar_theta: f32,
}

impl FindingDTO {
    /// Translate a domain finding and its projected coordinate into a DTO.
    pub fn from_finding(finding: &Finding, coordinate: PolarCoordinate) -> Self {
        Self {
            id: finding.id.clone(),
            file: finding.file_path.clone(),
            line_start: finding.line_start,
            line_end: finding.line_end,
            rule_id: finding.rule_id.clone(),
            category: finding.category.as_str().to_string(),
            severity: finding.severity.as_str().to_string(),
            cwe: finding.cwe.clone(),
            title: finding.title.clone(),
            description: finding.description.clone(),
            engine: finding.engine.as_str().to_string(),
            confidence: Some(finding.confidence),
            model_probability: finding.model_probability,
            heuristic_weight: finding.heuristic_weight,
            radar_r: coordinate.radius,
            radar_theta: coordinate.angle_deg,
        }
    }
}

/// One line edit of a patch, as it travels across the boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineEditDTO {
    /// First affected line, 1-based.
    pub start_line: u32,
    /// Last affected line, 1-based.
    pub end_line: u32,
    /// Byte offset (inclusive) of the replaced range.
    pub start_offset: usize,
    /// Byte offset (exclusive) of the replaced range.
    pub end_offset: usize,
    /// The replacement text.
    pub replacement: String,
}

impl LineEditDTO {
    /// Translate a domain edit into its boundary DTO.
    pub fn from_edit(edit: &LineEdit) -> Self {
        Self {
            start_line: edit.start_line,
            end_line: edit.end_line,
            start_offset: edit.start_offset,
            end_offset: edit.end_offset,
            replacement: edit.replacement.clone(),
        }
    }

    /// Translate the boundary DTO back into a domain edit.
    pub fn to_edit(&self) -> LineEdit {
        LineEdit::new(
            self.start_line,
            self.end_line,
            self.start_offset,
            self.end_offset,
            self.replacement.clone(),
        )
    }
}

/// The payload of a patch, as it travels from the preview to the applier.
///
/// It carries the fingerprint of the original file so the applier can detect a
/// file that changed in the meantime (`CR-SPEC-13 §4`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatchProposalDTO {
    /// Workspace-relative path of the patched file.
    pub file_path: String,
    /// The rule that motivated the change.
    pub rule_id: String,
    /// SHA-256 of the original file.
    pub original_sha256: String,
    /// Number of lines of the original file.
    pub original_line_count: usize,
    /// The edits that produce the new content.
    pub edits: Vec<LineEditDTO>,
    /// The resulting file content.
    pub new_content: String,
    /// The unified diff shown to the user.
    pub unified_diff: String,
    /// Why the remediation is safe.
    pub explanation: String,
}

impl PatchProposalDTO {
    /// Translate a domain proposal into its boundary DTO.
    pub fn from_proposal(proposal: &PatchProposal, unified_diff: String) -> Self {
        Self {
            file_path: proposal.file_path.clone(),
            rule_id: proposal.rule_id.clone(),
            original_sha256: proposal.original_sha256.clone(),
            original_line_count: proposal.original_line_count,
            edits: proposal.edits.iter().map(LineEditDTO::from_edit).collect(),
            new_content: proposal.new_content.clone(),
            unified_diff,
            explanation: proposal.explanation.clone(),
        }
    }

    /// Translate the boundary DTO back into a domain proposal.
    pub fn to_proposal(&self) -> PatchProposal {
        PatchProposal::new(
            self.file_path.clone(),
            self.rule_id.clone(),
            self.original_sha256.clone(),
            self.original_line_count,
            self.edits.iter().map(LineEditDTO::to_edit).collect(),
            self.new_content.clone(),
            self.explanation.clone(),
        )
    }
}

/// Everything the diff modal needs to review a proposed change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatchPreviewDTO {
    /// Workspace-relative path of the patched file.
    pub file_path: String,
    /// The rule that motivated the change.
    pub rule_id: String,
    /// Short title of the remediation.
    pub title: String,
    /// Why the remediation is safe.
    pub explanation: String,
    /// SHA-256 of the original file.
    pub original_sha256: String,
    /// SHA-256 of the patched file.
    pub new_sha256: String,
    /// Number of lines of the original file.
    pub original_line_count: usize,
    /// The unified diff shown to the user.
    pub unified_diff: String,
    /// Number of added lines.
    pub added_lines: usize,
    /// Number of removed lines.
    pub removed_lines: usize,
    /// Non-fatal warnings about the proposal.
    pub warnings: Vec<String>,
    /// The payload that is sent back when the user applies the patch.
    ///
    /// `None` when no deterministic fix exists for the finding: the
    /// remediation template did not match the code exactly, so no patch can be
    /// proposed. The rest of the preview still describes the finding and the
    /// reason why no remediation is available (`CR-SPEC-13 §5`).
    pub proposal: Option<PatchProposalDTO>,
}

/// The result of a patch attempt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatchOutcomeDTO {
    /// `applied`, `conflict` or `rejected`.
    pub status: String,
    /// Workspace-relative path of the patched file.
    pub file_path: String,
    /// Human readable explanation of the outcome.
    pub message: String,
    /// SHA-256 of the file before the patch.
    pub original_sha256: String,
    /// SHA-256 of the file after the patch, when it was applied.
    pub new_sha256: Option<String>,
}

impl PatchOutcomeDTO {
    /// Translate a domain outcome into its boundary DTO.
    pub fn from_outcome(outcome: &PatchOutcome) -> Self {
        match outcome {
            PatchOutcome::Applied {
                file_path,
                original_sha256,
                new_sha256,
            } => Self {
                status: outcome.status_token().to_string(),
                file_path: file_path.clone(),
                message: "the file was patched atomically".to_string(),
                original_sha256: original_sha256.clone(),
                new_sha256: Some(new_sha256.clone()),
            },
            PatchOutcome::Conflict {
                file_path,
                expected_sha256,
                actual_sha256,
            } => Self {
                status: outcome.status_token().to_string(),
                file_path: file_path.clone(),
                message: format!(
                    "the file changed on disk since the diff was produced (expected {expected_sha256}, found {actual_sha256}); nothing was written"
                ),
                original_sha256: expected_sha256.clone(),
                new_sha256: None,
            },
            PatchOutcome::Rejected { file_path, reason } => Self {
                status: outcome.status_token().to_string(),
                file_path: file_path.clone(),
                message: format!("the patch was refused: {reason}"),
                original_sha256: String::new(),
                new_sha256: None,
            },
        }
    }
}

/// One point of the quality trend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryPointDTO {
    /// Identifier of the scan session.
    pub id: String,
    /// The scanned workspace.
    pub root_dir: String,
    /// The scan mode.
    pub mode: String,
    /// Finish time as Unix milliseconds.
    pub finished_at_unix_ms: u64,
    /// Number of analysed files.
    pub files_scanned: usize,
    /// Total number of findings.
    pub total_findings: usize,
    /// Number of critical findings.
    pub critical: usize,
    /// Number of high findings.
    pub high: usize,
    /// Number of medium findings.
    pub medium: usize,
    /// Number of low findings.
    pub low: usize,
    /// Number of heuristic findings dropped by the precision floor.
    pub suppressed_heuristic_findings: usize,
    /// Engine label recorded for the session (`mock` | `jev` |
    /// `mock_fallback`), `None` when not declared.
    pub engine_mode: Option<String>,
    /// Semantic batch usage, absent when not recorded.
    #[serde(default)]
    pub usage: Option<ScanUsageDTO>,
    /// Compliance score of the scan, 0-100.
    pub compliance_percent: Option<u8>,
    /// File-level coverage; absent for legacy rows.
    pub coverage: Option<CoverageDTO>,
    /// Completion state, absent when not recorded.
    pub completion: Option<String>,
}

/// Scan-scoped semantic batch outcomes at the presentation boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanUsageDTO {
    /// Batches answered by the primary evaluator.
    pub primary_batches: u64,
    /// Batches answered by the fallback evaluator.
    pub fallback_batches: u64,
    /// Batches for which both evaluators failed.
    pub fallback_failures: u64,
}

impl From<ScanUsage> for ScanUsageDTO {
    fn from(value: ScanUsage) -> Self {
        Self {
            primary_batches: value.primary_batches,
            fallback_batches: value.fallback_batches,
            fallback_failures: value.fallback_failures,
        }
    }
}

/// File-level coverage at the presentation boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoverageDTO {
    /// Files discovered within the selected scope.
    pub discovered: usize,
    /// Files eligible for analysis.
    pub supported: usize,
    /// Files whose analysis completed.
    pub analyzed: usize,
    /// Unsupported files.
    pub unsupported: usize,
    /// Exclusions observed under the declared policy.
    pub excluded: usize,
    /// Whether policy exclusion accounting is complete, or unknown.
    #[serde(default)]
    pub exclusions_complete: Option<bool>,
    /// Files that could not be read.
    pub unreadable: usize,
    /// Files skipped for size.
    pub oversized: usize,
    /// Files skipped for invalid encoding.
    pub invalid_encoding: usize,
    /// Files with incomplete analysis.
    pub incomplete: usize,
}

impl From<ScanCoverage> for CoverageDTO {
    fn from(value: ScanCoverage) -> Self {
        Self {
            discovered: value.discovered,
            supported: value.supported,
            analyzed: value.analyzed,
            unsupported: value.unsupported,
            excluded: value.excluded,
            exclusions_complete: value.exclusions_complete,
            unreadable: value.unreadable,
            oversized: value.oversized,
            invalid_encoding: value.invalid_encoding,
            incomplete: value.incomplete,
        }
    }
}

impl HistoryPointDTO {
    /// Translate a persisted session summary into its boundary DTO.
    pub fn from_summary(summary: &ScanSessionSummary) -> Self {
        Self {
            id: summary.id.clone(),
            root_dir: summary.root_dir.clone(),
            mode: summary.mode.as_str().to_string(),
            finished_at_unix_ms: summary.finished_at_unix_ms,
            files_scanned: summary.files_scanned,
            total_findings: summary.total_findings,
            critical: summary.critical,
            high: summary.high,
            medium: summary.medium,
            low: summary.low,
            suppressed_heuristic_findings: summary.suppressed_heuristic_findings,
            engine_mode: summary.engine_mode.clone(),
            usage: summary.usage.map(ScanUsageDTO::from),
            compliance_percent: summary.compliance_percent,
            coverage: summary.coverage.map(CoverageDTO::from),
            completion: summary.completion.clone(),
        }
    }
}
