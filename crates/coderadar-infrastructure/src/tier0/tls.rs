//! `SEC-TLS-005` — Permissive TLS Cipher / Insecure Protocol (CWE-326).
//!
//! File/module scope, TypeScript/JavaScript only in v1.0. Flags plaintext
//! `http://` endpoints (localhost excluded), TLS 1.0/1.1 protocol pinning and
//! disabled certificate validation (`rejectUnauthorized: false`).

use std::sync::LazyLock;

use coderadar_domain::{ChunkKind, CodeChunk, Finding, RuleSpec};
use regex::Regex;

use crate::heuristics::{build_finding, chunk_lines};

/// Confidence of a plaintext `http://` URL.
const PLAINTEXT_HTTP_CONFIDENCE: f32 = 0.7;
/// Confidence of TLS 1.0/1.1 protocol pinning.
const TLS_VERSION_CONFIDENCE: f32 = 0.9;
/// Confidence of disabled certificate validation.
const CERT_VALIDATION_CONFIDENCE: f32 = 0.8;

/// Plaintext HTTP URLs; capture group 1 is the URL host (may be empty).
static PLAINTEXT_HTTP: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\bhttp://([A-Za-z0-9._\-]*)").expect("static plaintext-http pattern compiles")
});

/// `minVersion: "TLSv1"` / `"TLSv1.0"` / `"TLSv1.1"` pinning.
static TLS_MIN_VERSION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"minVersion\s*:\s*["']TLSv1(\.0|\.1)?["']"#)
        .expect("static minVersion pattern compiles")
});

/// Node `secureProtocol` pinning such as `"TLSv1_method"`.
static TLS_SECURE_PROTOCOL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"secureProtocol[^'"\n]*["']TLSv1"#)
        .expect("static secureProtocol pattern compiles")
});

/// nginx-style `ssl_protocols TLSv1 TLSv1.1;` directives.
///
/// The trailing separator group (instead of a plain `\b`) keeps TLS 1.2/1.3
/// directives clean: `ssl_protocols TLSv1.2 TLSv1.3;` must not be flagged.
static TLS_SSL_PROTOCOLS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"ssl_protocols[^;]*TLSv1(\.0|\.1)?(\s|;|$|["',)\]])"#)
        .expect("static ssl_protocols pattern compiles")
});

/// Disabled certificate validation.
static CERT_VALIDATION_DISABLED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"rejectUnauthorized\s*:\s*false")
        .expect("static rejectUnauthorized pattern compiles")
});

/// Evaluate `SEC-TLS-005` against whole-file ([`ChunkKind::FileModule`])
/// chunks. AST windows are ignored.
pub(crate) fn evaluate(rule: &RuleSpec, chunks: &[CodeChunk]) -> Vec<Finding> {
    let mut findings = Vec::new();
    for chunk in chunks
        .iter()
        .filter(|chunk| chunk.kind == ChunkKind::FileModule)
    {
        for (line_number, line) in chunk_lines(chunk) {
            let Some((description, confidence)) = violation(line) else {
                continue;
            };
            findings.push(build_finding(
                rule,
                &chunk.file_path,
                line_number,
                line_number,
                description,
                confidence,
            ));
        }
    }
    findings
}

/// The violation a single line carries, if any; at most one finding per line.
fn violation(line: &str) -> Option<(&'static str, f32)> {
    plaintext_http(line)
        .or_else(|| tls_versions(line))
        .or_else(|| certificate_validation_disabled(line))
}

/// Plaintext `http://` URL outside the local machine.
fn plaintext_http(line: &str) -> Option<(&'static str, f32)> {
    let captures = PLAINTEXT_HTTP.captures(line)?;
    let host = captures.get(1).map_or("", |capture| capture.as_str());
    if is_local_host(host) {
        return None;
    }
    Some(("insecure plaintext http:// URL", PLAINTEXT_HTTP_CONFIDENCE))
}

/// Hosts that legitimately speak plaintext HTTP during local development.
fn is_local_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost") || matches!(host, "127.0.0.1" | "0.0.0.0")
}

/// TLS 1.0/1.1 protocol pinning across the three configuration shapes.
fn tls_versions(line: &str) -> Option<(&'static str, f32)> {
    let pinned = TLS_MIN_VERSION.is_match(line)
        || TLS_SECURE_PROTOCOL.is_match(line)
        || TLS_SSL_PROTOCOLS.is_match(line);
    pinned.then_some(("TLS 1.0/1.1 protocol allowed", TLS_VERSION_CONFIDENCE))
}

/// Disabled certificate validation.
fn certificate_validation_disabled(line: &str) -> Option<(&'static str, f32)> {
    CERT_VALIDATION_DISABLED.is_match(line).then_some((
        "TLS certificate validation disabled",
        CERT_VALIDATION_CONFIDENCE,
    ))
}

#[cfg(test)]
mod tests {
    use coderadar_domain::{
        ChunkKind, CodeChunk, EvaluationEngine, FindingCategory, RuleScope, RuleSpec, Severity,
    };

    use super::evaluate;

    fn rule() -> RuleSpec {
        RuleSpec {
            id: "SEC-TLS-005".to_string(),
            title: "Permissive TLS Cipher / Insecure Protocol".to_string(),
            category: FindingCategory::Security,
            severity: Severity::Low,
            cwe: vec!["CWE-326".to_string()],
            scope: RuleScope::FileModule,
            engine: EvaluationEngine::Tier0Static,
            assertion: "Plaintext HTTP fallbacks or TLS 1.0/1.1".to_string(),
        }
    }

    fn evaluate_file(content: &str) -> Vec<coderadar_domain::Finding> {
        let chunk = CodeChunk::file_module(
            "src/client.ts",
            "typescript",
            content.lines().count() as u32,
            content,
        );
        evaluate(&rule(), &[chunk])
    }

    #[test]
    fn plaintext_http_urls_are_flagged_on_their_own_line() {
        let content = "const secure = \"https://api.example.com\";\nconst alt = \"http://api.example.com/v1\";\n";
        let findings = evaluate_file(content);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].line_start, 2);
        assert_eq!(findings[0].line_end, 2);
        assert!((findings[0].confidence - 0.7).abs() < f32::EPSILON);
        assert_eq!(findings[0].description, "insecure plaintext http:// URL");
    }

    #[test]
    fn localhost_http_urls_are_not_flagged() {
        let content = concat!(
            "const dev = \"http://localhost:3000/api\";\n",
            "const loopback = \"http://127.0.0.1:8080/health\";\n",
            "const any = \"http://0.0.0.0:4000\";\n",
            "const local = \"http://LOCALHOST/api\";\n",
        );
        let findings = evaluate_file(content);
        assert!(findings.is_empty(), "{findings:?}");
    }

    #[test]
    fn tls_10_and_11_pinning_is_flagged_on_its_own_line() {
        let content = concat!(
            "const agent = new https.Agent({\n",
            "  minVersion: \"TLSv1\",\n",
            "  secureProtocol: \"TLSv1_method\",\n",
            "});\n",
            "ssl_protocols TLSv1 TLSv1.1;\n",
        );
        let findings = evaluate_file(content);
        assert_eq!(findings.len(), 3, "{findings:?}");
        assert_eq!(findings[0].line_start, 2);
        assert_eq!(findings[1].line_start, 3);
        assert_eq!(findings[2].line_start, 5);
        for finding in &findings {
            assert!((finding.confidence - 0.9).abs() < f32::EPSILON);
            assert_eq!(finding.description, "TLS 1.0/1.1 protocol allowed");
        }
    }

    #[test]
    fn tls_12_and_13_configs_are_not_flagged() {
        let content = concat!(
            "const agent = new https.Agent({ minVersion: \"TLSv1.2\" });\n",
            "ssl_protocols TLSv1.2 TLSv1.3;\n",
        );
        let findings = evaluate_file(content);
        assert!(findings.is_empty(), "{findings:?}");
    }

    #[test]
    fn disabled_certificate_validation_is_flagged() {
        let content = "const agent = new https.Agent({\n  rejectUnauthorized: false,\n});\n";
        let findings = evaluate_file(content);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].line_start, 2);
        assert!((findings[0].confidence - 0.8).abs() < f32::EPSILON);
        assert_eq!(
            findings[0].description,
            "TLS certificate validation disabled"
        );
    }

    #[test]
    fn clean_https_file_produces_no_findings() {
        let content = concat!(
            "const base = \"https://api.example.com\";\n",
            "const agent = new https.Agent({ minVersion: \"TLSv1.2\", rejectUnauthorized: true });\n",
        );
        let findings = evaluate_file(content);
        assert!(findings.is_empty(), "{findings:?}");
    }

    #[test]
    fn ast_window_chunks_are_never_scanned() {
        let chunk = CodeChunk::new(
            "src/client.ts",
            "typescript",
            1,
            1,
            "const url = \"http://api.example.com\";\n",
        );
        assert_eq!(chunk.kind, ChunkKind::AstWindow);
        let findings = evaluate(&rule(), &[chunk]);
        assert!(findings.is_empty(), "{findings:?}");
    }
}
