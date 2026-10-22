//! Serializable scan report — the CLI's structured JSON output.
//!
//! Boundary DTOs only: the domain [`coderadar_domain::ScanSession`] and
//! `Finding` entities are translated here, never serialized directly
//! (`CR-SPEC-02 §3`). Field names are the stable CLI contract consumed by
//! tests, scripts and (from Phase 3 on) the Tauri command layer.

use serde::Serialize;

use coderadar_domain::{Finding, PullRequestHygiene, Severity};

use crate::layer_map::LayerMapSource;
use crate::pipeline::ScanOutcome;
use crate::rules_manifest::RuleManifest;

/// Tool identity block.
#[derive(Debug, Clone, Serialize)]
pub struct ToolInfo {
    /// Binary name.
    pub name: &'static str,
    /// Crate version.
    pub version: &'static str,
}

/// Scan statistics block.
#[derive(Debug, Clone, Serialize)]
pub struct ScanStatsReport {
    /// Supported files that passed the bloat guard.
    pub files_scanned: usize,
    /// AST-window chunks evaluated.
    pub ast_chunks: usize,
    /// Whole-file chunks evaluated.
    pub module_chunks: usize,
    /// Wall-clock duration of the scan.
    pub duration_ms: u64,
    /// Compliance score of the scan, 0-100.
    pub compliance_percent: u8,
    /// Number of heuristic findings dropped by the precision floor.
    pub suppressed_heuristic_findings: usize,
}

/// Findings counted by severity.
#[derive(Debug, Clone, Serialize)]
pub struct SeverityTotals {
    /// Total number of findings.
    pub total: usize,
    /// Critical findings.
    pub critical: usize,
    /// High findings.
    pub high: usize,
    /// Medium findings.
    pub medium: usize,
    /// Low findings.
    pub low: usize,
}

/// Session metadata block.
#[derive(Debug, Clone, Serialize)]
pub struct SessionReport {
    /// Stable session identifier.
    pub id: String,
    /// Root directory that was scanned.
    pub root_dir: String,
    /// Scan mode token (`full_scan`).
    pub mode: String,
    /// Start time as Unix milliseconds.
    pub started_at_unix_ms: u64,
    /// Finish time as Unix milliseconds.
    pub finished_at_unix_ms: u64,
    /// Severity totals.
    pub totals: SeverityTotals,
}

/// One serialized finding.
#[derive(Debug, Clone, Serialize)]
pub struct FindingReport {
    /// Stable finding id.
    pub id: String,
    /// Offending file (slash-separated, relative to the scan root).
    pub file: String,
    /// First offending line (1-based).
    pub line_start: u32,
    /// Last offending line (1-based).
    pub line_end: u32,
    /// Rule identifier.
    pub rule_id: String,
    /// Pillar token.
    pub category: String,
    /// Severity token.
    pub severity: String,
    /// CWE identifier, when applicable.
    pub cwe: Option<String>,
    /// Short title.
    pub title: String,
    /// Explanation (secrets are always masked).
    pub description: String,
    /// Evaluator confidence in `[0.0, 1.0]`.
    pub confidence: f32,
    /// Normalized risk score in `[0.0, 1.0]`.
    pub risk_score: f32,
}

/// Pull Request hygiene scorecard (present for `staged_diff` scans).
#[derive(Debug, Clone, Serialize)]
pub struct PrHygieneReport {
    /// The revision the staged index was compared against (`HEAD` or `empty`).
    pub base: String,
    /// Number of changed files.
    pub files_changed: usize,
    /// Number of added lines.
    pub additions: u32,
    /// Number of removed lines.
    pub deletions: u32,
    /// Total changed lines.
    pub lines_changed: u32,
    /// Changed production source files.
    pub production_files_changed: usize,
    /// Changed test files.
    pub test_files_changed: usize,
    /// Unrelated top-level areas touched, sorted.
    pub areas_touched: Vec<String>,
    /// `PR-SCP-001` fired.
    pub scope_creep: bool,
    /// `PR-BRK-002` fired.
    pub breaking_change: bool,
    /// `PR-TST-003` fired.
    pub missing_tests: bool,
}

/// The complete JSON document printed by `coderadar scan`.
#[derive(Debug, Clone, Serialize)]
pub struct ScanReport {
    /// Tool identity.
    pub tool: ToolInfo,
    /// Ruleset version the scan ran with.
    pub ruleset_version: String,
    /// Layer map provenance (`config_file` | `fallback`).
    pub layer_map_source: String,
    /// Scan statistics.
    pub scan: ScanStatsReport,
    /// Session metadata.
    pub session: SessionReport,
    /// Findings, sorted by file/line/rule for deterministic output.
    pub findings: Vec<FindingReport>,
    /// Pull Request hygiene, present only for a `staged_diff` scan.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pr_hygiene: Option<PrHygieneReport>,
}

impl ScanReport {
    /// Translate a completed [`ScanOutcome`] into the serializable report.
    pub fn from_scan(
        outcome: &ScanOutcome,
        manifest: &RuleManifest,
        layer_map_source: LayerMapSource,
        duration_ms: u64,
    ) -> Self {
        let session = &outcome.session;
        let summary = session.summary();
        let finished_at_unix_ms = session
            .finished_at_unix_ms
            .unwrap_or(session.started_at_unix_ms);

        Self {
            tool: ToolInfo {
                name: "coderadar",
                version: env!("CARGO_PKG_VERSION"),
            },
            ruleset_version: manifest.ruleset_version.clone(),
            layer_map_source: layer_map_source.as_str().to_string(),
            scan: ScanStatsReport {
                files_scanned: outcome.files_scanned,
                ast_chunks: outcome.ast_chunks,
                module_chunks: outcome.module_chunks,
                duration_ms,
                compliance_percent: summary.compliance_percent,
                suppressed_heuristic_findings: summary.suppressed_heuristic_findings,
            },
            session: SessionReport {
                id: session.id.clone(),
                root_dir: session.root_dir.clone(),
                mode: session.mode.as_str().to_string(),
                started_at_unix_ms: session.started_at_unix_ms,
                finished_at_unix_ms,
                totals: SeverityTotals {
                    total: session.findings.len(),
                    critical: session.count_severity(Severity::Critical),
                    high: session.count_severity(Severity::High),
                    medium: session.count_severity(Severity::Medium),
                    low: session.count_severity(Severity::Low),
                },
            },
            findings: session.findings.iter().map(finding_report).collect(),
            pr_hygiene: outcome.pull_request.as_ref().map(pr_hygiene_report),
        }
    }
}

/// Map the Pull Request hygiene assessment into its boundary DTO.
fn pr_hygiene_report(hygiene: &PullRequestHygiene) -> PrHygieneReport {
    PrHygieneReport {
        base: hygiene.base.clone(),
        files_changed: hygiene.files_changed,
        additions: hygiene.additions,
        deletions: hygiene.deletions,
        lines_changed: hygiene.lines_changed,
        production_files_changed: hygiene.production_files_changed,
        test_files_changed: hygiene.test_files_changed,
        areas_touched: hygiene.areas_touched.clone(),
        scope_creep: hygiene.scope_creep,
        breaking_change: hygiene.breaking_change,
        missing_tests: hygiene.missing_tests,
    }
}

/// Map one domain [`Finding`] into its boundary DTO.
fn finding_report(finding: &Finding) -> FindingReport {
    FindingReport {
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
        confidence: finding.confidence,
        risk_score: finding.risk_score.value(),
    }
}

#[cfg(test)]
mod tests {
    use coderadar_domain::{Finding, FindingCategory, RiskScore, ScanMode, ScanSession, Severity};

    use super::ScanReport;
    use crate::layer_map::LayerMapSource;
    use crate::pipeline::ScanOutcome;
    use crate::rules_manifest::RuleManifest;

    fn sample_outcome() -> ScanOutcome {
        let mut session = ScanSession::new("scan-1", "/tmp/root", ScanMode::FullScan, 1_000)
            .with_files_scanned(2)
            .with_suppressed_findings(2);
        session.push(
            Finding::new("SEC-TOK-002", "src/a.ts", 3, 3)
                .with_category(FindingCategory::Security)
                .with_severity(Severity::Critical)
                .with_cwe("CWE-798")
                .with_title("Hardcoded Secrets & API Keys")
                .with_description("found a thing [REDACTED]")
                .with_confidence(0.9)
                .with_risk_score(RiskScore::ONE),
        );
        session.finished_at_unix_ms = Some(1_500);
        ScanOutcome {
            session,
            files_scanned: 2,
            ast_chunks: 5,
            module_chunks: 2,
            pull_request: None,
        }
    }

    #[test]
    fn from_scan_maps_every_field() {
        let manifest = RuleManifest::embedded().expect("embedded manifest parses");
        let outcome = sample_outcome();
        let report = ScanReport::from_scan(&outcome, &manifest, LayerMapSource::ConfigFile, 42);

        let value: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&report).unwrap()).unwrap();

        assert_eq!(value["tool"]["name"], "coderadar");
        assert_eq!(value["tool"]["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(value["ruleset_version"], "2026.8.0-pr");
        assert_eq!(value["layer_map_source"], "config_file");

        assert_eq!(value["scan"]["files_scanned"], 2);
        assert_eq!(value["scan"]["ast_chunks"], 5);
        assert_eq!(value["scan"]["module_chunks"], 2);
        assert_eq!(value["scan"]["duration_ms"], 42);
        assert_eq!(value["scan"]["compliance_percent"], 50);
        assert_eq!(value["scan"]["suppressed_heuristic_findings"], 2);

        assert_eq!(value["session"]["id"], "scan-1");
        assert_eq!(value["session"]["root_dir"], "/tmp/root");
        assert_eq!(value["session"]["mode"], "full_scan");
        assert_eq!(value["session"]["started_at_unix_ms"], 1_000);
        assert_eq!(value["session"]["finished_at_unix_ms"], 1_500);
        assert_eq!(value["session"]["totals"]["total"], 1);
        assert_eq!(value["session"]["totals"]["critical"], 1);
        assert_eq!(value["session"]["totals"]["high"], 0);
        assert_eq!(value["session"]["totals"]["medium"], 0);
        assert_eq!(value["session"]["totals"]["low"], 0);

        let finding = &value["findings"][0];
        assert_eq!(finding["id"], "SEC-TOK-002:src/a.ts:3");
        assert_eq!(finding["file"], "src/a.ts");
        assert_eq!(finding["line_start"], 3);
        assert_eq!(finding["line_end"], 3);
        assert_eq!(finding["rule_id"], "SEC-TOK-002");
        assert_eq!(finding["category"], "security");
        assert_eq!(finding["severity"], "CRITICAL");
        assert_eq!(finding["cwe"], "CWE-798");
        assert_eq!(finding["title"], "Hardcoded Secrets & API Keys");
        assert_eq!(finding["description"], "found a thing [REDACTED]");
        let confidence = finding["confidence"].as_f64().expect("confidence number");
        assert!((confidence - 0.9).abs() < 0.001, "got {confidence}");
        assert_eq!(finding["risk_score"].as_f64(), Some(1.0));
    }

    #[test]
    fn unfinished_session_falls_back_to_started_time() {
        let manifest = RuleManifest::embedded().unwrap();
        let mut outcome = sample_outcome();
        outcome.session.finished_at_unix_ms = None;
        let report = ScanReport::from_scan(&outcome, &manifest, LayerMapSource::Fallback, 0);
        assert_eq!(report.session.finished_at_unix_ms, 1_000);
        assert_eq!(report.layer_map_source, "fallback");
    }
}
