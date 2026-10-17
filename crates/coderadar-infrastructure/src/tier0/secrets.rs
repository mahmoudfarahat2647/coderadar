//! `SEC-TOK-002` — Hardcoded Secrets & API Keys (CWE-798).
//!
//! AST-chunk scope. Two detection layers, both purely local:
//!
//! 1. **Signature patterns** — AWS access keys, Stripe live keys, GitHub /
//!    GitLab / Slack tokens, PEM private-key headers, JWTs.
//! 2. **Entropy heuristic** — quoted string literals assigned to a
//!    secret-flavoured identifier (`apiKey`, `secret`, `token`, `password`,
//!    ...) with Shannon entropy above a calibrated threshold.
//!
//! Findings never echo the secret: descriptions carry a masked preview
//! (see `crate::heuristics::mask_secret`).

use std::collections::HashMap;
use std::sync::LazyLock;

use coderadar_domain::{ChunkKind, CodeChunk, Finding, RuleSpec};
use regex::Regex;

use crate::heuristics::{build_finding, chunk_lines, mask_secret};

/// Confidence assigned to an exact credential-signature match.
const SIGNATURE_CONFIDENCE: f32 = 0.9;
/// Confidence assigned to the entropy heuristic.
const ENTROPY_CONFIDENCE: f32 = 0.6;
/// Minimum Shannon entropy (bits per character) for the entropy heuristic.
pub(crate) const ENTROPY_THRESHOLD: f64 = 3.5;

/// Case-insensitive marker substrings that identify placeholder values.
const PLACEHOLDER_MARKERS: [&str; 6] = [
    "example",
    "placeholder",
    "changeme",
    "dummy",
    "yourkey",
    "xxxxx",
];

/// A named credential-signature pattern; capture group 1 is the secret value.
///
/// Shared with the outbound redaction layer (`crate::redaction`), which reuses
/// exactly the same patterns so detection and scrubbing never diverge.
pub(crate) struct Signature {
    pub(crate) kind: &'static str,
    pub(crate) regex: Regex,
}

/// All credential-signature patterns, compiled once.
pub(crate) fn signatures() -> &'static [Signature] {
    static SIGNATURES: LazyLock<Vec<Signature>> = LazyLock::new(|| {
        vec![
            Signature {
                kind: "AWS access key",
                regex: Regex::new(r"(\bAKIA[0-9A-Z]{16}\b)")
                    .expect("static AWS access key pattern compiles"),
            },
            Signature {
                kind: "AWS secret access key",
                regex: Regex::new(
                    r#"(?i)aws_secret_access_key['"]?\s*[:=]\s*['"]([A-Za-z0-9/+=]{40})['"]"#,
                )
                .expect("static AWS secret access key pattern compiles"),
            },
            Signature {
                kind: "Stripe live secret key",
                regex: Regex::new(r"(\bsk_live_[0-9a-zA-Z]{10,}\b)")
                    .expect("static Stripe pattern compiles"),
            },
            Signature {
                kind: "GitHub token",
                regex: Regex::new(r"(\bgh[pousr]_[A-Za-z0-9]{20,}\b)")
                    .expect("static GitHub token pattern compiles"),
            },
            Signature {
                kind: "GitLab personal access token",
                regex: Regex::new(r"(\bglpat-[A-Za-z0-9_\-]{20,}\b)")
                    .expect("static GitLab token pattern compiles"),
            },
            Signature {
                kind: "Slack token",
                regex: Regex::new(r"(\bxox[baprs]-[A-Za-z0-9\-]{10,}\b)")
                    .expect("static Slack token pattern compiles"),
            },
            Signature {
                kind: "private key header",
                regex: Regex::new(r"(-----BEGIN [A-Z ]*PRIVATE KEY-----)")
                    .expect("static private key header pattern compiles"),
            },
            Signature {
                kind: "JWT literal",
                regex: Regex::new(
                    r"(\beyJ[A-Za-z0-9_\-]{10,}\.[A-Za-z0-9_\-]{10,}\.[A-Za-z0-9_\-]{10,}\b)",
                )
                .expect("static JWT pattern compiles"),
            },
        ]
    });
    SIGNATURES.as_slice()
}

/// Secret-flavoured assignment with a long quoted value: the entropy input.
///
/// The identifier part matches anywhere inside a mixed-case name (`jwtSecret`,
/// `dbPassword`), not only at a word start.
pub(crate) static ENTROPY_ASSIGNMENT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?i)\b\w*(?:api[_-]?key|secret|token|password|passwd|credential)\w*\s*[:=]\s*['"]([^'"]{16,})['"]"#,
    )
    .expect("static entropy-assignment pattern compiles")
});

/// Evaluate `SEC-TOK-002` against AST-window chunks.
pub(crate) fn evaluate(rule: &RuleSpec, chunks: &[CodeChunk]) -> Vec<Finding> {
    let mut findings = Vec::new();
    for chunk in chunks
        .iter()
        .filter(|chunk| chunk.kind == ChunkKind::AstWindow)
    {
        for (line_number, line) in chunk_lines(chunk) {
            if let Some(finding) = scan_line(rule, &chunk.file_path, line_number, line) {
                findings.push(finding);
            }
        }
    }
    findings
}

/// Check one line; signature matches take precedence over the entropy
/// heuristic so a line never yields two findings.
fn scan_line(rule: &RuleSpec, file_path: &str, line_number: u32, line: &str) -> Option<Finding> {
    for signature in signatures() {
        let Some(captures) = signature.regex.captures(line) else {
            continue;
        };
        let value = captures.get(1).or_else(|| captures.get(0))?;
        return Some(build_finding(
            rule,
            file_path,
            line_number,
            line_number,
            format!(
                "Hardcoded {} detected (value masked: {})",
                signature.kind,
                mask_secret(value.as_str())
            ),
            SIGNATURE_CONFIDENCE,
        ));
    }

    let captures = ENTROPY_ASSIGNMENT.captures(line)?;
    let value = captures.get(1)?.as_str();
    if is_placeholder(value) || shannon_entropy(value) < ENTROPY_THRESHOLD {
        return None;
    }
    Some(build_finding(
        rule,
        file_path,
        line_number,
        line_number,
        format!(
            "Hardcoded high-entropy secret detected (value masked: {})",
            mask_secret(value)
        ),
        ENTROPY_CONFIDENCE,
    ))
}

/// `true` when the value reads as a placeholder, never a real credential.
///
/// This filter applies to the entropy heuristic only: signature matches fire
/// even when their value contains words like `EXAMPLE` because fixtures
/// deliberately use fake-looking tokens that must still be caught.
pub(crate) fn is_placeholder(value: &str) -> bool {
    let lowercase = value.to_ascii_lowercase();
    PLACEHOLDER_MARKERS
        .iter()
        .any(|marker| lowercase.contains(marker))
}

/// Shannon entropy of `value` in bits per character.
pub(crate) fn shannon_entropy(value: &str) -> f64 {
    let mut counts: HashMap<char, usize> = HashMap::new();
    for character in value.chars() {
        *counts.entry(character).or_insert(0) += 1;
    }
    let length = value.chars().count() as f64;
    counts
        .values()
        .map(|count| {
            let probability = *count as f64 / length;
            -probability * probability.log2()
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use coderadar_domain::{
        ChunkKind, CodeChunk, EvaluationEngine, FindingCategory, RuleScope, RuleSpec, Severity,
    };

    use super::{evaluate, shannon_entropy};

    fn rule() -> RuleSpec {
        RuleSpec {
            id: "SEC-TOK-002".to_string(),
            title: "Hardcoded Secrets & API Keys".to_string(),
            category: FindingCategory::Security,
            severity: Severity::Critical,
            cwe: vec!["CWE-798".to_string()],
            scope: RuleScope::AstChunk,
            engine: EvaluationEngine::Tier0Static,
            assertion: "High-entropy strings matching API token signatures".to_string(),
        }
    }

    fn evaluate_content(content: &str) -> Vec<coderadar_domain::Finding> {
        let chunk = CodeChunk::new("src/config.ts", "typescript", 1, 1, content);
        evaluate(&rule(), &[chunk])
    }

    #[test]
    fn every_signature_pattern_fires_on_its_own_line() {
        let content = concat!(
            "const awsKey = \"AKIAIOSFODNN7EXAMPLE\";\n",
            "const awsSecret = { aws_secret_access_key: \"wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY\" };\n",
            "const stripe = \"sk_live_",
            "51H8xQy2eZvKYlo2CwNhLdQ9s\";\n",
            "const github = \"ghp_1234567890abcdefghijABCD\";\n",
            "const gitlab = \"glpat-abcdefghijklmnopqrst\";\n",
            "const slack = \"xoxb-1234567890-abcdefghij\";\n",
            "const pem = \"-----BEGIN RSA PRIVATE KEY-----\";\n",
            "const jwt = \"eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c\";\n",
        );
        let findings = evaluate_content(content);
        assert_eq!(findings.len(), 8, "{findings:#?}");
        for (index, finding) in findings.iter().enumerate() {
            assert_eq!(finding.line_start, index as u32 + 1, "{finding:?}");
            assert_eq!(finding.line_end, finding.line_start);
            assert!(
                (finding.confidence - 0.9).abs() < f32::EPSILON,
                "{finding:?}"
            );
            assert!(finding.description.contains("[REDACTED]"), "{finding:?}");
        }
        let expected_kinds = [
            "AWS access key",
            "AWS secret access key",
            "Stripe live secret key",
            "GitHub token",
            "GitLab personal access token",
            "Slack token",
            "private key header",
            "JWT literal",
        ];
        for (finding, kind) in findings.iter().zip(expected_kinds) {
            assert!(
                finding.description.contains(kind),
                "{}",
                finding.description
            );
        }
    }

    #[test]
    fn multi_line_chunks_report_absolute_line_numbers() {
        let content = "const harmless = 1;\nconst key = \"AKIAIOSFODNN7EXAMPLE\";\n";
        let chunk = CodeChunk::new("src/config.ts", "typescript", 10, 11, content);
        let findings = evaluate(&rule(), &[chunk]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].line_start, 11);
    }

    #[test]
    fn descriptions_never_echo_the_raw_secret() {
        let secret = "AKIAIOSFODNN7EXAMPLE";
        let findings = evaluate_content(&format!("const key = \"{secret}\";\n"));
        assert_eq!(findings.len(), 1);
        assert!(!findings[0].description.contains(secret));
    }

    #[test]
    fn entropy_heuristic_flags_high_entropy_assignments() {
        let content = "const jwtSecret = \"Zq4Lm9xT2vB8nR5s\";\n";
        let chunk = CodeChunk::new("src/config.ts", "typescript", 5, 5, content);
        let findings = evaluate(&rule(), &[chunk]);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].line_start, 5);
        assert!((findings[0].confidence - 0.6).abs() < f32::EPSILON);
        assert!(findings[0].description.contains("high-entropy secret"));
        assert!(findings[0].description.contains("[REDACTED]"));
    }

    #[test]
    fn entropy_heuristic_ignores_low_entropy_and_placeholder_values() {
        for content in [
            "const apiKey = \"aaaaaaaaaaaaaaaa\";\n",
            "const apiKey = \"your_api_key_here\";\n",
            "const apiKey = \"YOURKEY1234567890abcdef\";\n",
            "const apiKey = \"short\";\n",
        ] {
            let findings = evaluate_content(content);
            assert!(findings.is_empty(), "{content}: {findings:?}");
        }
    }

    #[test]
    fn placeholder_filter_never_applies_to_signature_matches() {
        let findings = evaluate_content("const key = \"AKIAIOSFODNN7EXAMPLE\";\n");
        assert_eq!(findings.len(), 1, "{findings:?}");
    }

    #[test]
    fn a_line_matching_signature_and_entropy_yields_one_signature_finding() {
        let findings = evaluate_content("const token = \"ghp_1234567890abcdefghijABCD\";\n");
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!((findings[0].confidence - 0.9).abs() < f32::EPSILON);
        assert!(findings[0].description.contains("GitHub token"));
    }

    #[test]
    fn clean_code_produces_no_findings() {
        let content = "export function add(a: number, b: number): number {\n  return a + b;\n}\n";
        let findings = evaluate_content(content);
        assert!(findings.is_empty(), "{findings:?}");
    }

    #[test]
    fn file_module_chunks_are_never_scanned() {
        let chunk = CodeChunk::file_module(
            "src/config.ts",
            "typescript",
            1,
            "const key = \"AKIAIOSFODNN7EXAMPLE\";\n",
        );
        assert_eq!(chunk.kind, ChunkKind::FileModule);
        let findings = evaluate(&rule(), &[chunk]);
        assert!(findings.is_empty(), "{findings:?}");
    }

    #[test]
    fn shannon_entropy_is_bits_per_character() {
        assert!((shannon_entropy("aaaaaaaaaaaaaaaa")).abs() < f64::EPSILON);
        assert!((shannon_entropy("Zq4Lm9xT2vB8nR5s") - 4.0).abs() < 1e-9);
    }
}
