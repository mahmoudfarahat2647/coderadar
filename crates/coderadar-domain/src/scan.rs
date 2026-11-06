use crate::finding::Finding;
use crate::severity::Severity;
use std::collections::BTreeMap;

/// How a scan was triggered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ScanMode {
    /// Walk the whole project tree.
    #[default]
    FullScan,
    /// Inspect only the Git staged diff.
    StagedDiff,
}

impl ScanMode {
    /// Canonical token used across boundaries.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FullScan => "full_scan",
            Self::StagedDiff => "staged_diff",
        }
    }
}

/// File-level scan accounting. Excluded directories represent policy scope and
/// are not recursively counted. Unsupported files are not included in supported.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScanCoverage {
    /// Discovered file candidates, excluding pruned directories.
    pub discovered: usize,
    /// Supported file candidates.
    pub supported: usize,
    /// Supported files whose planned analysis completed.
    pub analyzed: usize,
    /// Files with unsupported languages.
    pub unsupported: usize,
    /// Observed excluded policy entries, without recursive counts.
    pub excluded: usize,
    /// Whether all exclusions in the selected scope were observed; None means unknown.
    pub exclusions_complete: Option<bool>,
    /// Supported files that could not be read, plus walk errors.
    pub unreadable: usize,
    /// Supported files exceeding the byte or line length limit.
    pub oversized: usize,
    /// Supported files that are not UTF-8.
    pub invalid_encoding: usize,
    /// Supported files whose analysis has not yet been confirmed by the pipeline.
    pub incomplete: usize,
}

/// Cumulative semantic batch outcomes for a single scan.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScanUsage {
    /// Batches answered by the primary evaluator.
    pub primary_batches: u64,
    /// Batches answered by the fallback evaluator.
    pub fallback_batches: u64,
    /// Batches for which both evaluators failed.
    pub fallback_failures: u64,
}

/// A complete scan run and its findings.
#[derive(Debug, Clone, PartialEq)]
pub struct ScanSession {
    /// Stable session identifier.
    pub id: String,
    /// Root directory that was scanned.
    pub root_dir: String,
    /// How the scan was triggered.
    pub mode: ScanMode,
    /// Engine label recorded by the composition root (`mock` | `jev` |
    /// `mock_fallback`), `None` when the caller does not declare one.
    pub engine_mode: Option<String>,
    /// Scan-scoped semantic usage, or unknown when not collected.
    pub usage: Option<ScanUsage>,
    /// Start time as Unix milliseconds.
    pub started_at_unix_ms: u64,
    /// Finish time as Unix milliseconds, `None` while still running.
    pub finished_at_unix_ms: Option<u64>,
    /// Number of files that were actually analysed.
    pub files_scanned: usize,
    /// File-level coverage, or unknown for legacy sessions.
    pub coverage: Option<ScanCoverage>,
    /// Recorded completion state, or unknown when not recorded.
    pub completion: Option<String>,
    /// SHA-256 fingerprints of analyzed source files, or unknown for older sessions.
    pub analyzed_sources: Option<BTreeMap<String, String>>,
    /// Fingerprints of all consumed sources, including those invalidated during the scan.
    pub consumed_sources: Option<BTreeMap<String, String>>,
    /// Heuristic findings dropped by the precision floor
    /// (`CR-SPEC-09 §5`): they were produced, but not reported.
    pub suppressed_heuristic_findings: usize,
    /// Findings produced by the scan.
    pub findings: Vec<Finding>,
}

/// Compact, persistence-friendly view of a finished scan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanSessionSummary {
    /// Identifier of the originating session.
    pub id: String,
    /// Root directory that was scanned.
    pub root_dir: String,
    /// How the scan was triggered.
    pub mode: ScanMode,
    /// Finish time as Unix milliseconds.
    pub finished_at_unix_ms: u64,
    /// Number of files that were analysed.
    pub files_scanned: usize,
    /// File-level coverage, or unknown for legacy sessions.
    pub coverage: Option<ScanCoverage>,
    /// Recorded completion state, or unknown for legacy sessions.
    pub completion: Option<String>,
    /// Fingerprints of analyzed sources, unknown for legacy sessions.
    pub analyzed_sources: Option<BTreeMap<String, String>>,
    /// Fingerprints of consumed sources, unknown for legacy sessions.
    pub consumed_sources: Option<BTreeMap<String, String>>,
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
    /// Scan-scoped semantic usage, or unknown when not collected.
    pub usage: Option<ScanUsage>,
    /// Compliance score of the scan, 0-100 (files without findings).
    pub compliance_percent: Option<u8>,
}

impl ScanSession {
    /// Start a new scan session.
    pub fn new(
        id: impl Into<String>,
        root_dir: impl Into<String>,
        mode: ScanMode,
        started_at_unix_ms: u64,
    ) -> Self {
        Self {
            id: id.into(),
            root_dir: root_dir.into(),
            mode,
            engine_mode: None,
            usage: None,
            started_at_unix_ms,
            finished_at_unix_ms: None,
            files_scanned: 0,
            coverage: None,
            completion: None,
            analyzed_sources: None,
            consumed_sources: None,
            suppressed_heuristic_findings: 0,
            findings: Vec::new(),
        }
    }

    /// Record how many files were analysed.
    pub fn with_files_scanned(mut self, files_scanned: usize) -> Self {
        self.files_scanned = files_scanned;
        self
    }

    /// Record how many heuristic findings the precision floor dropped
    /// (`CR-SPEC-09 §5`).
    pub fn with_suppressed_heuristic_findings(mut self, suppressed: usize) -> Self {
        self.suppressed_heuristic_findings = suppressed;
        self
    }

    /// Record the engine label of the composition root that drove the scan.
    pub fn with_engine_mode(mut self, engine_mode: impl Into<String>) -> Self {
        self.engine_mode = Some(engine_mode.into());
        self
    }

    /// Add a finding to the session.
    pub fn push(&mut self, finding: Finding) {
        self.findings.push(finding);
    }

    /// Count findings at a given severity.
    pub fn count_severity(&self, severity: Severity) -> usize {
        self.findings
            .iter()
            .filter(|finding| finding.severity == severity)
            .count()
    }

    /// Compliance score of the session, 0-100.
    ///
    /// A file *passes* when it carries no finding at all. The score is the
    /// share of passing files among the analysed files, rounded to the nearest
    /// percent. A scan without any analysed file has no score.
    pub fn compliance_percent(&self) -> Option<u8> {
        if self.files_scanned == 0 {
            return None;
        }
        let mut failing: Vec<&str> = self
            .findings
            .iter()
            .filter(|finding| {
                self.analyzed_sources
                    .as_ref()
                    .is_none_or(|sources| sources.contains_key(&finding.file_path))
            })
            .map(|finding| finding.file_path.as_str())
            .collect();
        failing.sort_unstable();
        failing.dedup();
        let failing = failing.len().min(self.files_scanned);
        let passing = self.files_scanned - failing;
        Some(((passing * 100 + self.files_scanned / 2) / self.files_scanned) as u8)
    }

    /// Reduce the session to a persistence-friendly summary.
    pub fn summary(&self) -> ScanSessionSummary {
        ScanSessionSummary {
            id: self.id.clone(),
            root_dir: self.root_dir.clone(),
            mode: self.mode,
            finished_at_unix_ms: self.finished_at_unix_ms.unwrap_or(self.started_at_unix_ms),
            files_scanned: self.files_scanned,
            coverage: self.coverage,
            completion: self.completion.clone(),
            analyzed_sources: self.analyzed_sources.clone(),
            consumed_sources: self.consumed_sources.clone(),
            total_findings: self.findings.len(),
            critical: self.count_severity(Severity::Critical),
            high: self.count_severity(Severity::High),
            medium: self.count_severity(Severity::Medium),
            low: self.count_severity(Severity::Low),
            suppressed_heuristic_findings: self.suppressed_heuristic_findings,
            engine_mode: self.engine_mode.clone(),
            usage: self.usage,
            compliance_percent: self.compliance_percent(),
        }
    }
}
