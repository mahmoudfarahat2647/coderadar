use crate::finding::Finding;
use crate::severity::Severity;

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

/// A complete scan run and its findings.
#[derive(Debug, Clone, PartialEq)]
pub struct ScanSession {
    /// Stable session identifier.
    pub id: String,
    /// Root directory that was scanned.
    pub root_dir: String,
    /// How the scan was triggered.
    pub mode: ScanMode,
    /// Start time as Unix milliseconds.
    pub started_at_unix_ms: u64,
    /// Finish time as Unix milliseconds, `None` while still running.
    pub finished_at_unix_ms: Option<u64>,
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
            started_at_unix_ms,
            finished_at_unix_ms: None,
            findings: Vec::new(),
        }
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

    /// Reduce the session to a persistence-friendly summary.
    pub fn summary(&self) -> ScanSessionSummary {
        ScanSessionSummary {
            id: self.id.clone(),
            root_dir: self.root_dir.clone(),
            mode: self.mode,
            finished_at_unix_ms: self.finished_at_unix_ms.unwrap_or(self.started_at_unix_ms),
            total_findings: self.findings.len(),
            critical: self.count_severity(Severity::Critical),
            high: self.count_severity(Severity::High),
            medium: self.count_severity(Severity::Medium),
            low: self.count_severity(Severity::Low),
        }
    }
}
