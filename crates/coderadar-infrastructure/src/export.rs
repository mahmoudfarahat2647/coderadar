//! Markdown export of a scan report (`FR-07`).
//!
//! The rendering itself lives in Layer 3 ([`coderadar_adapters`]); this module
//! only adapts the infrastructure report to the renderer input and writes the
//! resulting document to disk.
//!
//! The write is **atomic**: the Markdown lands in a staging file next to the
//! target and is then `rename`d over it, so a reader never sees a half
//! written report (`CR-SPEC-13 §4`).

use std::path::{Path, PathBuf};

use coderadar_adapters::{render_markdown_report, MarkdownFinding, MarkdownReportInput};
use coderadar_domain::DomainError;

use crate::patching::atomic_write;
use crate::report::ScanReport;

/// Default file name of an exported report.
pub const DEFAULT_REPORT_FILE_NAME: &str = "report.md";

/// The result of an export.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportOutcome {
    /// Absolute path of the written report.
    pub path: String,
    /// Size of the report in bytes.
    pub bytes: usize,
    /// Number of findings the report describes.
    pub findings: usize,
}

/// Render the Markdown report of a finished scan.
///
/// The result is deterministic: the same scan always yields the same bytes.
pub fn markdown_from_report(report: &ScanReport) -> String {
    let input = MarkdownReportInput {
        tool_name: report.tool.name.to_string(),
        tool_version: report.tool.version.to_string(),
        ruleset_version: report.ruleset_version.clone(),
        layer_map_source: report.layer_map_source.clone(),
        session_id: report.session.id.clone(),
        root_dir: report.session.root_dir.clone(),
        mode: report.session.mode.clone(),
        started_at_unix_ms: report.session.started_at_unix_ms,
        finished_at_unix_ms: report.session.finished_at_unix_ms,
        files_scanned: report.scan.files_scanned,
        ast_chunks: report.scan.ast_chunks,
        module_chunks: report.scan.module_chunks,
        duration_ms: report.scan.duration_ms,
        compliance_percent: report.scan.compliance_percent,
        findings: report
            .findings
            .iter()
            .map(|finding| MarkdownFinding {
                file: finding.file.clone(),
                line_start: finding.line_start,
                line_end: finding.line_end,
                rule_id: finding.rule_id.clone(),
                category: finding.category.clone(),
                severity: finding.severity.clone(),
                cwe: finding.cwe.clone(),
                title: finding.title.clone(),
                description: finding.description.clone(),
                confidence: finding.confidence,
                risk_score: finding.risk_score,
            })
            .collect(),
    };

    render_markdown_report(&input).markdown
}

/// Write the Markdown report of a scan to `path`.
///
/// The write is atomic: the document is staged next to the target and then
/// renamed over it, so the target is never observed half written. Missing
/// parent directories are created.
///
/// # Errors
/// Returns [`DomainError::Io`] when the report cannot be written.
pub fn write_markdown_report(
    path: &Path,
    report: &ScanReport,
) -> Result<ExportOutcome, DomainError> {
    let markdown = markdown_from_report(report);
    write_markdown_text(path, &markdown, report.findings.len())
}

/// Write an already rendered Markdown document to `path`.
///
/// # Errors
/// Returns [`DomainError::Io`] when the document cannot be written.
fn write_markdown_text(
    path: &Path,
    markdown: &str,
    findings: usize,
) -> Result<ExportOutcome, DomainError> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        if !parent.exists() {
            std::fs::create_dir_all(parent).map_err(|error| {
                DomainError::Io(format!(
                    "cannot create the directory `{}`: {error}",
                    parent.display()
                ))
            })?;
        }
    }

    atomic_write(path, markdown)?;

    Ok(ExportOutcome {
        path: absolute_display(path),
        bytes: markdown.len(),
        findings,
    })
}

/// Render a path for the user, preferring the absolute form.
fn absolute_display(path: &Path) -> String {
    let absolute: PathBuf = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    absolute.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::{markdown_from_report, write_markdown_report};
    use crate::layer_map::LayerMapSource;
    use crate::pipeline::ScanOutcome;
    use crate::report::ScanReport;
    use crate::rules_manifest::RuleManifest;
    use coderadar_domain::{Finding, FindingCategory, RiskScore, ScanMode, ScanSession, Severity};

    fn report_with_two_findings() -> ScanReport {
        let mut session = ScanSession::new("scan-1", "/tmp/root", ScanMode::FullScan, 1_000)
            .with_files_scanned(4);
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
        session.push(
            Finding::new("ARCH-DEP-001", "src/b.ts", 7, 7)
                .with_category(FindingCategory::CleanArchitecture)
                .with_severity(Severity::High)
                .with_title("Forbidden dependency")
                .with_description("the inner layer imports an outer crate")
                .with_confidence(0.8)
                .with_risk_score(RiskScore::new(0.75).unwrap()),
        );
        session.finished_at_unix_ms = Some(1_500);
        let outcome = ScanOutcome {
            session,
            files_scanned: 4,
            ast_chunks: 8,
            module_chunks: 4,
        };
        let manifest = RuleManifest::embedded().expect("embedded manifest parses");
        ScanReport::from_scan(&outcome, &manifest, LayerMapSource::ConfigFile, 500)
    }

    #[test]
    fn the_markdown_contains_the_summary_and_the_findings() {
        let report = report_with_two_findings();
        assert_eq!(report.scan.compliance_percent, 50);
        let markdown = markdown_from_report(&report);
        assert!(markdown.contains("## Executive summary"));
        assert!(markdown.contains("| Findings | 2 |"));
        assert!(markdown.contains(&format!(
            "| Compliance | {} % |",
            report.scan.compliance_percent
        )));
        assert!(markdown.contains("SEC-TOK-002"));
        assert!(markdown.contains("ARCH-DEP-001"));
        assert!(markdown.contains("CWE-798"));
    }

    #[test]
    fn the_report_is_written_atomically() {
        let directory = tempfile::TempDir::new().expect("temp dir");
        let target = directory.path().join("report.md");
        let outcome = write_markdown_report(&target, &report_with_two_findings())
            .expect("the report is written");

        assert_eq!(outcome.findings, 2);
        assert!(outcome.bytes > 0);
        let written = std::fs::read_to_string(&target).expect("the report is readable");
        assert!(written.contains("## Executive summary"));

        // No staging file is left behind.
        let leftovers: Vec<_> = std::fs::read_dir(directory.path())
            .expect("the directory is readable")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.contains("coderadar"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "leftover staging files: {leftovers:?}"
        );
    }

    #[test]
    fn a_missing_directory_is_created() {
        let directory = tempfile::TempDir::new().expect("temp dir");
        let target = directory.path().join("nested").join("report.md");
        write_markdown_report(&target, &report_with_two_findings()).expect("the report is written");
        assert!(target.exists());
    }
}
