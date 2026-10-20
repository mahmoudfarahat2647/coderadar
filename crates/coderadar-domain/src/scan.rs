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
    /// Number of files that were actually analysed.
    pub files_scanned: usize,
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
    /// Compliance score of the scan, 0-100 (files without findings).
    pub compliance_percent: u8,
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
            files_scanned: 0,
            findings: Vec::new(),
        }
    }

    /// Record how many files were analysed.
    pub fn with_files_scanned(mut self, files_scanned: usize) -> Self {
        self.files_scanned = files_scanned;
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
    /// percent. A scan without any analysed file scores 100.
    pub fn compliance_percent(&self) -> u8 {
        if self.files_scanned == 0 {
            return 100;
        }
        let mut failing: Vec<&str> = self
            .findings
            .iter()
            .map(|finding| finding.file_path.as_str())
            .collect();
        failing.sort_unstable();
        failing.dedup();
        let failing = failing.len().min(self.files_scanned);
        let passing = self.files_scanned - failing;
        ((passing * 100 + self.files_scanned / 2) / self.files_scanned) as u8
    }

    /// Reduce the session to a persistence-friendly summary.
    pub fn summary(&self) -> ScanSessionSummary {
        ScanSessionSummary {
            id: self.id.clone(),
            root_dir: self.root_dir.clone(),
            mode: self.mode,
            finished_at_unix_ms: self.finished_at_unix_ms.unwrap_or(self.started_at_unix_ms),
            files_scanned: self.files_scanned,
            total_findings: self.findings.len(),
            critical: self.count_severity(Severity::Critical),
            high: self.count_severity(Severity::High),
            medium: self.count_severity(Severity::Medium),
            low: self.count_severity(Severity::Low),
            compliance_percent: self.compliance_percent(),
        }
    }
}
