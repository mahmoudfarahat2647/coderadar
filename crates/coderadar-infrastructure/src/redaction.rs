//! Tier 0 pre-flight secret redaction — the privacy guard (`CR-SPEC-03 §3`).
//!
//! Before any code leaves the machine, every credential-shaped literal is
//! replaced with [`REDACTED_SECRET`]. The scrubber reuses the exact Tier 0
//! detectors from [`crate::tier0::secrets`] (signature patterns + the
//! secret-flavoured assignment regex), so what the scanner can flag is what
//! the redactor will hide — the two can never diverge.
//!
//! # Conservative by design
//!
//! Redaction favours **recall over precision**: a signature match is always
//! scrubbed (even `AKIA…EXAMPLE` fixtures), and any value assigned to a
//! secret-flavoured identifier with Shannon entropy at or above the Tier 0
//! threshold ([`ENTROPY_THRESHOLD`]) is scrubbed unless it is an obvious
//! placeholder. A false positive only costs a little semantic context; a false
//! negative leaks a live credential.

use coderadar_domain::CodeChunk;

use crate::tier0::secrets::{
    is_placeholder, shannon_entropy, signatures, ENTROPY_ASSIGNMENT, ENTROPY_THRESHOLD,
};

/// The literal that replaces every detected secret.
pub const REDACTED_SECRET: &str = "[REDACTED_SECRET]";

/// Redact every detected secret in `source`, preserving line structure.
///
/// Returns the scrubbed text; it never truncates or reflows the source, so
/// line numbers computed by the AST chunker stay valid.
pub fn redact_source(source: &str) -> String {
    source
        .split('\n')
        .map(redact_line)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Redact a chunk, returning a copy with a scrubbed `content` payload.
///
/// The chunk's location metadata (`file_path`, `language`, `kind`, line range)
/// is preserved verbatim so the evaluator reports findings against the real
/// source coordinates.
#[must_use]
pub fn redact_chunk(chunk: &CodeChunk) -> CodeChunk {
    let mut redacted = chunk.clone();
    redacted.content = redact_source(&chunk.content);
    redacted
}

/// Redact all secret ranges on a single line.
fn redact_line(line: &str) -> String {
    let ranges = secret_ranges(line);
    if ranges.is_empty() {
        return line.to_string();
    }

    let mut output = String::with_capacity(line.len());
    let mut cursor = 0;
    for (start, end) in merge_ranges(ranges) {
        output.push_str(&line[cursor..start]);
        output.push_str(REDACTED_SECRET);
        cursor = end;
    }
    output.push_str(&line[cursor..]);
    output
}

/// Byte ranges of every secret literal on `line` (signature + entropy rules).
fn secret_ranges(line: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();

    for signature in signatures() {
        for captures in signature.regex.captures_iter(line) {
            if let Some(value) = captures.get(1).or_else(|| captures.get(0)) {
                ranges.push((value.start(), value.end()));
            }
        }
    }

    for captures in ENTROPY_ASSIGNMENT.captures_iter(line) {
        let Some(value) = captures.get(1) else {
            continue;
        };
        if !is_placeholder(value.as_str()) && shannon_entropy(value.as_str()) >= ENTROPY_THRESHOLD {
            ranges.push((value.start(), value.end()));
        }
    }

    ranges
}

/// Sort ranges and collapse overlaps so an index is never rewritten twice.
fn merge_ranges(mut ranges: Vec<(usize, usize)>) -> Vec<(usize, usize)> {
    ranges.sort_unstable();
    let mut merged: Vec<(usize, usize)> = Vec::with_capacity(ranges.len());
    for (start, end) in ranges {
        match merged.last_mut() {
            Some(last) if start <= last.1 => last.1 = last.1.max(end),
            _ => merged.push((start, end)),
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use coderadar_domain::{ChunkKind, CodeChunk};

    use super::{redact_chunk, redact_source, REDACTED_SECRET};

    #[test]
    fn signature_secrets_are_replaced() {
        let source = concat!(
            "const aws = \"AKIAIOSFODNN7EXAMPLE\";\n",
            "const stripe = \"sk_live_51H8xQy2eZvKYlo2CwNhLdQ9s\";\n",
            "const github = \"ghp_1234567890abcdefghijABCD\";\n",
        );
        let redacted = redact_source(source);
        assert!(!redacted.contains("AKIAIOSFODNN7EXAMPLE"), "{redacted}");
        assert!(!redacted.contains("sk_live_"), "{redacted}");
        assert!(!redacted.contains("ghp_"), "{redacted}");
        assert_eq!(redacted.matches(REDACTED_SECRET).count(), 3, "{redacted}");
    }

    #[test]
    fn secret_flavoured_assignments_are_replaced() {
        let source =
            "const jwtSecret = \"Zq4Lm9xT2vB8nR5s\";\nconst dbPassword = \"aB3dE6gH9kL2mN5p\";\n";
        let redacted = redact_source(source);
        assert!(!redacted.contains("Zq4Lm9xT2vB8nR5s"), "{redacted}");
        assert!(!redacted.contains("aB3dE6gH9kL2mN5p"), "{redacted}");
        assert_eq!(redacted.matches(REDACTED_SECRET).count(), 2, "{redacted}");
    }

    #[test]
    fn placeholders_and_short_values_are_untouched() {
        let source = concat!(
            "const apiKey = \"your_api_key_here\";\n",
            "const token = \"short\";\n",
            "const label = \"a benign value\";\n",
        );
        assert_eq!(redact_source(source), source);
    }

    #[test]
    fn redaction_preserves_line_count_and_clean_lines() {
        let source = "export function add(a, b) {\n  const key = \"AKIAIOSFODNN7EXAMPLE\";\n  return a + b;\n}\n";
        let redacted = redact_source(source);
        assert_eq!(redacted.lines().count(), source.lines().count());
        assert!(redacted.contains("export function add(a, b) {"));
        assert!(redacted.contains("return a + b;"));
        assert!(redacted.contains(REDACTED_SECRET));
    }

    #[test]
    fn only_the_secret_slice_of_a_line_is_replaced() {
        let redacted = redact_source("const key = \"AKIAIOSFODNN7EXAMPLE\"; // keep me\n");
        assert!(
            redacted.starts_with("const key = \"[REDACTED_SECRET]\";"),
            "{redacted}"
        );
        assert!(redacted.contains("// keep me"), "{redacted}");
    }

    #[test]
    fn redact_chunk_keeps_location_metadata() {
        let chunk = CodeChunk::new(
            "src/config.ts",
            "typescript",
            10,
            11,
            "const harmless = 1;\nconst key = \"AKIAIOSFODNN7EXAMPLE\";\n",
        );
        let redacted = redact_chunk(&chunk);
        assert_eq!(redacted.file_path, "src/config.ts");
        assert_eq!(redacted.language, "typescript");
        assert_eq!(redacted.kind, ChunkKind::AstWindow);
        assert_eq!(redacted.start_line, 10);
        assert_eq!(redacted.end_line, 11);
        assert!(!redacted.content.contains("AKIAIOSFODNN7EXAMPLE"));
        assert!(redacted.content.contains(REDACTED_SECRET));
    }

    #[test]
    fn private_key_headers_are_redacted() {
        let redacted = redact_source("const pem = \"-----BEGIN RSA PRIVATE KEY-----\";\n");
        assert!(redacted.contains(REDACTED_SECRET), "{redacted}");
        assert!(!redacted.contains("BEGIN RSA PRIVATE KEY"), "{redacted}");
    }

    #[test]
    fn clean_source_is_returned_unchanged() {
        let source = "export function add(a: number, b: number): number {\n  return a + b;\n}\n";
        assert_eq!(redact_source(source), source);
    }
}
