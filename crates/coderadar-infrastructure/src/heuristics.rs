//! Shared helpers for heuristic rule implementations (Tier 0 and Mock).
//!
//! Centralises the mapping from a [`RuleSpec`] to a fully-populated
//! [`Finding`] so every engine emits findings with identical metadata
//! semantics: severity, category, CWE and title always come from the ruleset
//! manifest, never from the heuristic itself.

use coderadar_domain::{CodeChunk, Finding, RiskScore, RuleSpec};

/// Build a finding whose rule metadata (severity, category, CWE, title) is
/// taken from the manifest-backed [`RuleSpec`].
pub(crate) fn build_finding(
    rule: &RuleSpec,
    file_path: &str,
    line_start: u32,
    line_end: u32,
    description: impl Into<String>,
    confidence: f32,
) -> Finding {
    let mut finding = Finding::new(rule.id.clone(), file_path, line_start, line_end)
        .with_category(rule.category)
        .with_severity(rule.severity)
        .with_title(rule.title.clone())
        .with_description(description)
        .with_confidence(confidence)
        .with_risk_score(RiskScore::from(rule.severity));
    if let Some(cwe) = rule.primary_cwe() {
        finding = finding.with_cwe(cwe);
    }
    finding
}

/// Iterate a chunk as `(absolute_line_number, line_text)` pairs.
pub(crate) fn chunk_lines(chunk: &CodeChunk) -> impl Iterator<Item = (u32, &str)> {
    chunk
        .content
        .lines()
        .enumerate()
        .map(move |(index, line)| (chunk.start_line + index as u32, line))
}

/// Mask a detected secret so it is never echoed in full into a report.
pub(crate) fn mask_secret(value: &str) -> String {
    let visible: String = value.chars().take(4).collect();
    format!("{visible}\u{2026}[REDACTED]")
}

#[cfg(test)]
mod tests {
    use super::{build_finding, chunk_lines, mask_secret};
    use coderadar_domain::{CodeChunk, FindingCategory, RuleScope, Severity};
    use coderadar_domain::{EvaluationEngine, RuleSpec};

    fn rule() -> RuleSpec {
        RuleSpec {
            id: "SEC-TOK-002".to_string(),
            title: "Hardcoded Secrets & API Keys".to_string(),
            category: FindingCategory::Security,
            severity: Severity::Critical,
            cwe: vec!["CWE-798".to_string()],
            scope: RuleScope::AstChunk,
            engine: EvaluationEngine::Tier0Static,
            assertion: "test".to_string(),
        }
    }

    #[test]
    fn finding_inherits_rule_metadata() {
        let finding = build_finding(&rule(), "a.ts", 3, 3, "desc", 0.9);
        assert_eq!(finding.rule_id, "SEC-TOK-002");
        assert_eq!(finding.severity, Severity::Critical);
        assert_eq!(finding.category, FindingCategory::Security);
        assert_eq!(finding.cwe.as_deref(), Some("CWE-798"));
        assert_eq!(finding.title, "Hardcoded Secrets & API Keys");
        assert_eq!(finding.risk_score.value(), 1.0);
        assert!((finding.confidence - 0.9).abs() < f32::EPSILON);
        assert_eq!(finding.id, "SEC-TOK-002:a.ts:3");
    }

    #[test]
    fn chunk_lines_offsets_by_start_line() {
        let chunk = CodeChunk::new("a.ts", "typescript", 10, 12, "one\ntwo\nthree\n");
        let lines: Vec<(u32, &str)> = chunk_lines(&chunk).collect();
        assert_eq!(lines, vec![(10, "one"), (11, "two"), (12, "three")]);
    }

    #[test]
    fn mask_keeps_four_prefix_chars() {
        assert_eq!(
            mask_secret("AKIAIOSFODNN7EXAMPLE"),
            "AKIA\u{2026}[REDACTED]"
        );
        assert_eq!(mask_secret("ab"), "ab\u{2026}[REDACTED]");
    }
}
