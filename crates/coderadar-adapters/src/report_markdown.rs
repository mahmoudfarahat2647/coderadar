//! Markdown report presenter.
//!
//! Turns a finished scan into a structured, human-readable Markdown document
//! (`FR-07`): an executive summary, the radar metrics and the findings grouped
//! by pillar (`CR-SPEC-01 §3`).
//!
//! Like [`crate::diff`], this module is pure text processing: it never touches
//! the filesystem and never sees an inner entity. The caller hands over a flat
//! [`MarkdownReportInput`], the renderer returns a deterministic document — the
//! same scan always produces byte-identical Markdown.

use coderadar_domain::FindingCategory;

/// One finding, as it appears in the report.
///
/// This is a boundary DTO: inner entities are never rendered directly
/// (`CR-SPEC-02 §3`).
#[derive(Debug, Clone, PartialEq)]
pub struct MarkdownFinding {
    /// Workspace-relative path of the offending file.
    pub file: String,
    /// First offending line (1-based).
    pub line_start: u32,
    /// Last offending line (1-based).
    pub line_end: u32,
    /// Rule that produced the finding.
    pub rule_id: String,
    /// Pillar token (`security`, `architecture`, …).
    pub category: String,
    /// Severity token (`CRITICAL`, `HIGH`, …).
    pub severity: String,
    /// Associated CWE, when applicable.
    pub cwe: Option<String>,
    /// Short title.
    pub title: String,
    /// Detailed explanation (secrets are always masked).
    pub description: String,
    /// Evaluator confidence in `[0.0, 1.0]`.
    pub confidence: f32,
    /// Model probability, when available.
    pub model_probability: Option<f32>,
    /// Heuristic weight, when available.
    pub heuristic_weight: Option<f32>,
    /// Normalized risk score in `[0.0, 1.0]`.
    pub risk_score: f32,
}

/// Everything the renderer needs to describe a scan.
#[derive(Debug, Clone, PartialEq)]
pub struct MarkdownReportInput {
    /// Tool name.
    pub tool_name: String,
    /// Tool version.
    pub tool_version: String,
    /// Version of the ruleset the scan ran with.
    pub ruleset_version: String,
    /// Layer-map provenance (`config_file` | `fallback`).
    pub layer_map_source: String,
    /// Identifier of the scan session.
    pub session_id: String,
    /// Root directory that was scanned.
    pub root_dir: String,
    /// Scan mode token (`full_scan`).
    pub mode: String,
    /// Engine mode as recorded by the scan.
    pub engine_mode: String,
    /// Cumulative primary, fallback, and failed batch counts, if recorded.
    pub usage: Option<(u64, u64, u64)>,
    /// Start time as Unix milliseconds.
    pub started_at_unix_ms: u64,
    /// Finish time as Unix milliseconds.
    pub finished_at_unix_ms: u64,
    /// Number of analysed files.
    pub files_scanned: usize,
    /// Number of AST-window chunks.
    pub ast_chunks: usize,
    /// Number of whole-file chunks.
    pub module_chunks: usize,
    /// Wall-clock duration of the scan.
    pub duration_ms: u64,
    /// Compliance score of the scan, 0-100.
    pub compliance_percent: Option<u8>,
    /// Scan completion state, when recorded.
    pub completion: Option<String>,
    /// Analyzed source paths and SHA-256 fingerprints, if recorded.
    pub analyzed_sources: Option<Vec<(String, String)>>,
    /// Consumed source paths and fingerprints, including invalidated sources, if recorded.
    pub consumed_sources: Option<Vec<(String, String)>>,
    /// Named file-level coverage counts, or `None` when not recorded.
    pub coverage: Option<Vec<(&'static str, usize)>>,
    /// Number of heuristic findings the precision floor dropped and which are
    /// therefore only counted (`CR-SPEC-09 §5`).
    pub suppressed_heuristic_findings: usize,
    /// The findings of the scan.
    pub findings: Vec<MarkdownFinding>,
}

/// The rendered Markdown document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownReport {
    /// The Markdown text.
    pub markdown: String,
    /// Size of the document in bytes.
    pub byte_len: usize,
    /// Number of findings the document describes.
    pub finding_count: usize,
}

/// Severity buckets, ordered from the most to the least severe.
const SEVERITIES: [&str; 4] = ["CRITICAL", "HIGH", "MEDIUM", "LOW"];

/// Pillars, in the canonical order of the radar quadrants.
fn pillars() -> [(&'static str, &'static str, u32); 5] {
    [
        FindingCategory::CleanArchitecture,
        FindingCategory::Security,
        FindingCategory::EdgeCases,
        FindingCategory::Performance,
        FindingCategory::PullRequests,
    ]
    .map(|category| {
        (
            category.as_str(),
            category.pillar_name(),
            category.radar_base_angle_deg() as u32,
        )
    })
}

/// Severity badge used in the summary table.
fn severity_badge(severity: &str) -> &'static str {
    match severity {
        "CRITICAL" => "🔴 CRITICAL",
        "HIGH" => "🟠 HIGH",
        "MEDIUM" => "🟡 MEDIUM",
        "LOW" => "🔵 LOW",
        _ => "⚪ CLEAN",
    }
}

/// Rank of a severity token; lower is more severe.
fn severity_rank(severity: &str) -> usize {
    SEVERITIES
        .iter()
        .position(|token| *token == severity)
        .unwrap_or(SEVERITIES.len())
}

/// Render the Markdown report of a scan.
///
/// The document is deterministic: the findings are sorted by severity, then by
/// pillar, then by file and line, so two runs over the same tree produce the
/// same bytes.
pub fn render_markdown_report(input: &MarkdownReportInput) -> MarkdownReport {
    let findings = sorted_findings(&input.findings);
    let mut out = String::with_capacity(4096);

    render_header(&mut out, input);
    render_executive_summary(&mut out, input, &findings);
    render_radar_metrics(&mut out, &findings);
    render_findings(&mut out, &findings);
    render_footer(&mut out, input);

    MarkdownReport {
        byte_len: out.len(),
        finding_count: findings.len(),
        markdown: out,
    }
}

/// Sort the findings by severity, pillar, file and line.
fn sorted_findings(findings: &[MarkdownFinding]) -> Vec<MarkdownFinding> {
    let mut sorted = findings.to_vec();
    sorted.sort_by(|left, right| {
        severity_rank(&left.severity)
            .cmp(&severity_rank(&right.severity))
            .then_with(|| pillar_rank(&left.category).cmp(&pillar_rank(&right.category)))
            .then_with(|| left.file.cmp(&right.file))
            .then_with(|| left.line_start.cmp(&right.line_start))
            .then_with(|| left.rule_id.cmp(&right.rule_id))
    });
    sorted
}

/// Rank of a pillar token; lower comes first in the report.
fn pillar_rank(category: &str) -> usize {
    pillars()
        .iter()
        .position(|(token, _, _)| *token == category)
        .unwrap_or(pillars().len())
}

/// Render the title block and the scan identity.
fn render_header(out: &mut String, input: &MarkdownReportInput) {
    out.push_str("# CodeRadar Studio — Scan Report\n\n");
    out.push_str(&format!(
        "> **Tool** {} {} · **Ruleset** `{}` · **Session** `{}`\n",
        escape_text(&input.tool_name),
        escape_text(&input.tool_version),
        escape_text(&input.ruleset_version),
        escape_text(&input.session_id)
    ));
    out.push_str(&format!(
        "> **Workspace** `{}` · **Mode** `{}` · **Layer map** `{}`\n",
        escape_text(&input.root_dir),
        escape_text(&input.mode),
        escape_text(&input.layer_map_source)
    ));
    out.push_str(&format!(
        "> **Started** {} · **Finished** {}\n\n",
        format_timestamp(input.started_at_unix_ms),
        format_timestamp(input.finished_at_unix_ms)
    ));
}

/// Render the executive summary: the scan metrics and the severity split.
fn render_executive_summary(
    out: &mut String,
    input: &MarkdownReportInput,
    findings: &[MarkdownFinding],
) {
    out.push_str("## Executive summary\n\n");
    out.push_str("| Metric | Value |\n|---|---|\n");
    out.push_str(&format!("| Files scanned | {} |\n", input.files_scanned));
    out.push_str(&format!("| AST chunks | {} |\n", input.ast_chunks));
    out.push_str(&format!("| Module chunks | {} |\n", input.module_chunks));
    out.push_str(&format!("| Duration | {} ms |\n", input.duration_ms));
    out.push_str(&format!(
        "| Engine mode | {} |\n",
        escape_text(&input.engine_mode)
    ));
    for (label, count) in [
        ("Primary batches", input.usage.map(|usage| usage.0)),
        ("Fallback batches", input.usage.map(|usage| usage.1)),
        ("Failed batches", input.usage.map(|usage| usage.2)),
    ] {
        out.push_str(&format!(
            "| {label} | {} |\n",
            count
                .map(|value| value.to_string())
                .unwrap_or_else(|| "Unknown".to_string())
        ));
    }
    out.push_str(&format!("| Findings | {} |\n", findings.len()));
    out.push_str(&format!(
        "| Completion | {} |\n",
        input
            .completion
            .as_deref()
            .map(escape_text)
            .unwrap_or_else(|| "Unknown".to_string())
    ));
    out.push_str(&format!(
        "| Compliance | {} |\n",
        input
            .compliance_percent
            .map(|score| format!("{score} %"))
            .unwrap_or_else(|| "Unavailable".to_string())
    ));
    out.push_str(&format!(
        "| Average risk score | {} |\n",
        format_score(mean_risk(findings))
    ));
    out.push_str(&format!(
        "| Suppressed low-precision findings | {} |\n",
        input.suppressed_heuristic_findings
    ));
    out.push('\n');
    out.push_str("Compliance is the share of analyzed files without reported findings, not a security certification.\n\n");
    if let Some(coverage) = &input.coverage {
        out.push_str("### File coverage\n\n| Measure | Files |\n|---|---:|\n");
        for (label, count) in coverage {
            out.push_str(&format!("| {label} | {count} |\n"));
        }
        out.push_str("\nExclusion accounting may be incomplete or unknown as indicated above; excluded directories are not traversed just to count their contents.\n\n");
        if coverage
            .iter()
            .any(|(label, count)| *label == "Incomplete" && *count > 0)
        {
            out.push_str("Some supported files had incomplete analysis; findings and scores do not cover those files.\n\n");
        }
    } else {
        out.push_str("Coverage not recorded.\n\n");
    }

    if let Some(sources) = &input.analyzed_sources {
        out.push_str("### Analyzed source identities\n\n| Source | SHA-256 |\n|---|---|\n");
        for (path, hash) in sources {
            out.push_str(&format!(
                "| {} | {} |\n",
                escape_text(path),
                escape_text(hash)
            ));
        }
        out.push('\n');
    } else {
        out.push_str("Source identities not recorded.\n\n");
    }

    if let Some(sources) = &input.consumed_sources {
        out.push_str("### Consumed source identities\n\n| Source | SHA-256 |\n|---|---|\n");
        for (path, hash) in sources {
            out.push_str(&format!(
                "| {} | {} |\n",
                escape_text(path),
                escape_text(hash)
            ));
        }
        out.push('\n');
    } else {
        out.push_str("Consumed source identities not recorded.\n\n");
    }

    out.push_str("### Severity distribution\n\n");
    out.push_str("| Severity | Count | Share |\n|---|---:|---:|\n");
    let total = findings.len();
    for severity in SEVERITIES {
        let count = findings
            .iter()
            .filter(|finding| finding.severity == severity)
            .count();
        let share = if total == 0 {
            0.0
        } else {
            (count as f64) * 100.0 / (total as f64)
        };
        out.push_str(&format!(
            "| {} | {} | {} |\n",
            severity_badge(severity),
            count,
            format_share(share)
        ));
    }
    out.push('\n');
}

/// Render the radar metrics: one row per pillar quadrant.
fn render_radar_metrics(out: &mut String, findings: &[MarkdownFinding]) {
    out.push_str("### Radar metrics\n\n");
    out.push_str(
        "The 360° polar radar plots one node per actionable finding: the radius is the risk score \
         and the angle is the pillar quadrant.\n\n",
    );
    out.push_str("| Pillar | Quadrant | Findings | Mean risk | Max risk |\n");
    out.push_str("|---|---:|---:|---:|---:|\n");
    for (token, pillar, angle) in pillars() {
        let scoped: Vec<&MarkdownFinding> = findings
            .iter()
            .filter(|finding| finding.category == token)
            .collect();
        if scoped.is_empty() {
            continue;
        }
        let mean = scoped.iter().map(|f| f.risk_score).sum::<f32>() / scoped.len() as f32;
        let max = scoped
            .iter()
            .map(|f| f.risk_score)
            .fold(0.0_f32, |acc, score| acc.max(score));
        out.push_str(&format!(
            "| {} | {}° | {} | {} | {} |\n",
            escape_text(pillar),
            angle,
            scoped.len(),
            format_score(mean),
            format_score(max)
        ));
    }
    out.push('\n');
}

/// Render the findings, grouped by pillar.
fn render_findings(out: &mut String, findings: &[MarkdownFinding]) {
    out.push_str("## Findings by category\n\n");
    if findings.is_empty() {
        out.push_str("No findings were reported. Consult coverage for analysis limitations.\n\n");
        return;
    }

    for (token, pillar, _) in pillars() {
        let scoped: Vec<&MarkdownFinding> = findings
            .iter()
            .filter(|finding| finding.category == token)
            .collect();
        if scoped.is_empty() {
            continue;
        }
        out.push_str(&format!(
            "### {} ({})\n\n",
            escape_text(pillar),
            scoped.len()
        ));
        for (index, finding) in scoped.iter().enumerate() {
            render_finding(out, index + 1, finding);
        }
        out.push('\n');
    }
}

/// Render one finding as a subsection.
fn render_finding(out: &mut String, index: usize, finding: &MarkdownFinding) {
    out.push_str(&format!(
        "#### {}. `{}` — {} ({})\n\n",
        index,
        escape_text(&finding.rule_id),
        escape_text(&finding.title),
        finding.severity
    ));

    let location = if finding.line_start == finding.line_end {
        format!("{}:{}", escape_text(&finding.file), finding.line_start)
    } else {
        format!(
            "{}:{}-{}",
            escape_text(&finding.file),
            finding.line_start,
            finding.line_end
        )
    };
    out.push_str(&format!("- **Location:** `{location}`\n"));
    if let Some(cwe) = finding.cwe.as_deref().filter(|cwe| !cwe.is_empty()) {
        out.push_str(&format!("- **CWE:** {}\n", escape_text(cwe)));
    }
    out.push_str(&format!(
        "- **Risk score:** {} · **Confidence:** {}\n",
        format_score(finding.risk_score),
        format_score(finding.confidence)
    ));
    if let Some(probability) = finding.model_probability {
        out.push_str(&format!(
            "- **Model probability:** {}\n",
            format_score(probability)
        ));
    }
    if let Some(weight) = finding.heuristic_weight {
        out.push_str(&format!(
            "- **Heuristic weight:** {}\n",
            format_score(weight)
        ));
    }
    out.push('\n');

    let description = escape_text(&finding.description);
    if !description.is_empty() {
        out.push_str(&format!("> {description}\n\n"));
    }
}

/// Render the closing block with the reproducibility information.
fn render_footer(out: &mut String, input: &MarkdownReportInput) {
    out.push_str("---\n\n");
    out.push_str(&format!(
        "*Generated by {} {} with the ruleset `{}` — {} finding(s), {} file(s) analysed.*\n",
        escape_text(&input.tool_name),
        escape_text(&input.tool_version),
        escape_text(&input.ruleset_version),
        input.findings.len(),
        input.files_scanned
    ));
}

/// Mean risk score of the findings, `0.0` when there is none.
fn mean_risk(findings: &[MarkdownFinding]) -> f32 {
    if findings.is_empty() {
        return 0.0;
    }
    findings.iter().map(|f| f.risk_score).sum::<f32>() / findings.len() as f32
}

/// Format a score with two decimals.
fn format_score(score: f32) -> String {
    format!("{score:.2}")
}

/// Format a share with one decimal and a percent sign.
fn format_share(share: f64) -> String {
    format!("{share:.1} %")
}

/// Escape the characters that would break a Markdown table cell.
fn escape_text(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '|' => escaped.push_str("\\|"),
            '\n' | '\r' => escaped.push(' '),
            '`' => escaped.push_str("\\`"),
            _ => escaped.push(character),
        }
    }
    escaped
}

/// Format a Unix timestamp as an ISO-8601 UTC instant.
///
/// The renderer has no dependency on a date-time crate, so the civil date is
/// derived from the day number with the standard proleptic Gregorian
/// algorithm.
pub fn format_timestamp(unix_ms: u64) -> String {
    let total_seconds = (unix_ms / 1000) as i64;
    let days = total_seconds.div_euclid(86_400);
    let seconds_of_day = total_seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        seconds_of_day / 3600,
        (seconds_of_day % 3600) / 60,
        seconds_of_day % 60
    )
}

/// Convert a day number since the Unix epoch into a civil `(year, month, day)`.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = if shifted >= 0 {
        shifted
    } else {
        shifted - 146_096
    } / 146_097;
    let day_of_era = (shifted - era * 146_097) as u64;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era as i64 + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_index + 2) / 5 + 1) as u32;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    } as u32;
    let year = if month <= 2 { year + 1 } else { year };
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::{format_timestamp, render_markdown_report, MarkdownFinding, MarkdownReportInput};

    fn finding(severity: &str, category: &str, file: &str, line: u32) -> MarkdownFinding {
        MarkdownFinding {
            file: file.to_string(),
            line_start: line,
            line_end: line,
            rule_id: "SEC-TOK-002".to_string(),
            category: category.to_string(),
            severity: severity.to_string(),
            cwe: Some("CWE-798".to_string()),
            title: "Hardcoded Secrets & API Keys".to_string(),
            description: "a credential is embedded in the source".to_string(),
            confidence: 0.9,
            model_probability: None,
            heuristic_weight: None,
            risk_score: 1.0,
        }
    }

    fn input(findings: Vec<MarkdownFinding>) -> MarkdownReportInput {
        MarkdownReportInput {
            tool_name: "coderadar".to_string(),
            tool_version: "1.0.0".to_string(),
            ruleset_version: "2026.8.0-pr".to_string(),
            layer_map_source: "config_file".to_string(),
            session_id: "scan-1".to_string(),
            root_dir: "/tmp/root".to_string(),
            mode: "full_scan".to_string(),
            engine_mode: "unknown".to_string(),
            usage: None,
            started_at_unix_ms: 1_000,
            finished_at_unix_ms: 1_500,
            files_scanned: 12,
            ast_chunks: 30,
            module_chunks: 12,
            duration_ms: 500,
            compliance_percent: Some(75),
            completion: None,
            coverage: None,
            analyzed_sources: None,
            consumed_sources: None,
            suppressed_heuristic_findings: 3,
            findings,
        }
    }

    #[test]
    fn the_report_carries_an_executive_summary_and_the_findings() {
        let report = render_markdown_report(&input(vec![finding(
            "CRITICAL",
            "security",
            "src/api.ts",
            10,
        )]));

        assert!(report
            .markdown
            .starts_with("# CodeRadar Studio — Scan Report\n"));
        assert!(report.markdown.contains("## Executive summary"));
        assert!(report.markdown.contains("### Severity distribution"));
        assert!(report.markdown.contains("### Radar metrics"));
        assert!(report.markdown.contains("## Findings by category"));
        assert!(report.markdown.contains("### Security & Compliance (1)"));
        assert!(report.markdown.contains("`SEC-TOK-002`"));
        assert!(report.markdown.contains("`src/api.ts:10`"));
        assert!(report.markdown.contains("CWE-798"));
        assert!(report
            .markdown
            .contains("| Suppressed low-precision findings | 3 |"));
        assert_eq!(report.finding_count, 1);
        assert_eq!(report.byte_len, report.markdown.len());
    }

    #[test]
    fn a_clean_scan_says_so() {
        let report = render_markdown_report(&input(Vec::new()));
        assert!(report.markdown.contains("No findings were reported."));
        assert!(report.markdown.contains("| Findings | 0 |"));
        assert_eq!(report.finding_count, 0);
    }

    #[test]
    fn the_findings_are_sorted_by_severity() {
        let mut low = finding("LOW", "performance", "src/a.ts", 1);
        low.rule_id = "PERF-NPO-001".to_string();
        let report = render_markdown_report(&input(vec![
            low,
            finding("CRITICAL", "security", "src/b.ts", 2),
        ]));

        let critical = report.markdown.find("CRITICAL").expect("critical present");
        let low_position = report.markdown.find("PERF-NPO-001").expect("low present");
        assert!(
            critical < low_position,
            "the critical finding must be rendered first"
        );
    }

    #[test]
    fn the_rendering_is_deterministic() {
        let findings = vec![
            finding("HIGH", "architecture", "src/a.ts", 1),
            finding("CRITICAL", "security", "src/b.ts", 2),
        ];
        let first = render_markdown_report(&input(findings.clone()));
        let second = render_markdown_report(&input(findings));
        assert_eq!(first.markdown, second.markdown);
    }

    #[test]
    fn markdown_characters_in_the_text_are_escaped() {
        let mut hostile = finding("HIGH", "security", "src/a.ts", 1);
        hostile.title = "Pipe | and `backtick`".to_string();
        hostile.description = "first line\nsecond line".to_string();
        let report = render_markdown_report(&input(vec![hostile]));

        assert!(report.markdown.contains("Pipe \\| and \\`backtick\\`"));
        assert!(report.markdown.contains("first line second line"));
        assert!(!report.markdown.contains("first line\nsecond line"));
    }

    #[test]
    fn timestamps_are_rendered_as_utc_instants() {
        assert_eq!(format_timestamp(0), "1970-01-01T00:00:00Z");
        assert_eq!(format_timestamp(1_000), "1970-01-01T00:00:01Z");
        // 2026-10-21T10:00:00Z
        assert_eq!(format_timestamp(1_792_576_800_000), "2026-10-21T10:00:00Z");
    }
}
