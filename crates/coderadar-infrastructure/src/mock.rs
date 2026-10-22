//! Mock heuristic rule evaluator (`CR-SPEC-03 §4`).
//!
//! Offline stand-in for the Jev Tier 1 semantic engine: evaluates the rule
//! matrix with deterministic regex heuristics and emulates the typed-decision
//! pipeline shape (one [`RuleEvaluationResult`] per requested rule).
//!
//! Rules owned by the Tier 0 engine are never evaluated here. The
//! `git_changeset`-scoped `PR-*` rules return empty results until the Git
//! changeset engine lands in Phase 8. Synthetic latency is configurable and
//! disabled by default so CLI scans stay instant; the dashboard enables it in
//! Phase 3 to animate the radar sweep.
//!
//! # Heuristic matrix (`CR-SPEC-04 §3–§5`)
//!
//! Every pattern emits at most one finding per line, with `line_start ==
//! line_end`, absolute line numbers, and a short generic description that
//! never echoes the source. AST-scope rules only read [`ChunkKind::AstWindow`]
//! chunks; file/module-scope rules only read [`ChunkKind::FileModule`] chunks.
//!
//! | Rule | Trigger | Confidence |
//! | --- | --- | :--: |
//! | `SEC-INJ-001` | SQL keyword next to a `+`-concatenated literal, or interpolation in a db/shell call | 0.65 |
//! | `SEC-JWT-003` | `jwt.decode(` line, or `algorithms` configured with a quoted `none` | 0.75 / 0.9 |
//! | `SEC-XSS-004` | `dangerouslySetInnerHTML`, `.innerHTML =`, `document.write(` | 0.7 |
//! | `ARCH-SRP-002` | class of 200+ lines mixing persistence, network or UI concerns | 0.5 |
//! | `ARCH-DIP-003` | concrete adapter construction in `domain/` or use-case code | 0.55 |
//! | `ARCH-BND-004` | route/controller file responding with a domain entity or model | 0.6 |
//! | `EDGE-NPE-001` | unguarded nested property access on request, `.find` or storage results | 0.55 |
//! | `EDGE-PRM-002` | `.then(` without `.catch(`, or `fetch(` without catch/try handling | 0.5 |
//! | `EDGE-BND-003` | variable index without `.length` or `Array.isArray` in the chunk | 0.4 |
//! | `EDGE-TMO-004` | `fetch`/`axios` call without timeout, signal or abort in the chunk | 0.6 |
//! | `PERF-NPO-001` | database call inside a loop | 0.55 |
//! | `PERF-HDL-002` | handle opening without close, end, release or `finally` | 0.6 |
//! | `PERF-EVT-003` | `addEventListener(` without removal; `useEffect(` without cleanup | 0.6 / 0.5 |

use std::sync::LazyLock;
use std::time::Duration;

use coderadar_domain::{
    ChunkKind, CodeChunk, DomainError, EvaluationEngine, Finding, RuleEvaluationResult, RuleSpec,
};
use coderadar_usecases::{BoxFuture, SemanticRuleEvaluator};
use regex::Regex;

use crate::engine_rules::{GIT_CHANGESET_IDS, TIER0_OWNED_IDS};
use crate::heuristics::{build_finding, chunk_lines};

/// Minimum chunk length (lines) before `ARCH-SRP-002` may flag a class.
const SRP_MIN_LINES: u32 = 200;

// Description catalogue: short, generic trigger shapes. They never echo source
// text or secrets and always carry the Mock-engine verification notice.
const DESC_INJ_CONCAT: &str =
    "Heuristic: SQL keyword concatenated with a string (Mock engine — verify before acting)";
const DESC_INJ_TEMPLATE: &str =
    "Heuristic: interpolated template in a db/shell call (Mock engine — verify before acting)";
const DESC_JWT_DECODE: &str =
    "Heuristic: JWT decoded without signature verification (Mock engine — verify before acting)";
const DESC_JWT_NONE: &str =
    "Heuristic: JWT verification accepts `none` (Mock engine — verify before acting)";
const DESC_XSS: &str = "Heuristic: unsanitized HTML sink (Mock engine — verify before acting)";
const DESC_SRP: &str =
    "Heuristic: oversized class mixing concern groups (Mock engine — verify before acting)";
const DESC_DIP: &str =
    "Heuristic: inner layer instantiates a concrete adapter (Mock engine — verify before acting)";
const DESC_BND: &str =
    "Heuristic: route handler returns a domain entity (Mock engine — verify before acting)";
const DESC_NPE: &str =
    "Heuristic: unguarded access on a possibly-null value (Mock engine — verify before acting)";
const DESC_PRM_THEN: &str =
    "Heuristic: promise chain without catch handling (Mock engine — verify before acting)";
const DESC_PRM_FETCH: &str =
    "Heuristic: fetch without catch or try handling (Mock engine — verify before acting)";
const DESC_IDX: &str =
    "Heuristic: variable index without a length check (Mock engine — verify before acting)";
const DESC_TMO: &str =
    "Heuristic: network call without timeout or abort signal (Mock engine — verify before acting)";
const DESC_NPO: &str =
    "Heuristic: database call inside a loop (Mock engine — verify before acting)";
const DESC_HDL: &str =
    "Heuristic: handle opened without a guaranteed release (Mock engine — verify before acting)";
const DESC_EVT_ADD: &str =
    "Heuristic: event listener without a matching removal (Mock engine — verify before acting)";
const DESC_EVT_EFFECT: &str =
    "Heuristic: effect hook without cleanup (Mock engine — verify before acting)";

/// `SEC-INJ-001`: SQL keywords used to spot inline query strings.
static SQL_KEYWORD_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:SELECT|INSERT|UPDATE|DELETE|DROP)\b").expect("valid SQL keyword regex")
});

/// `SEC-INJ-001`: a quoted literal immediately concatenated with `+`.
static SQL_STRING_CONCAT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"['"][^'"\r\n]*['"]\s*\+"#).expect("valid string concat regex"));

/// `SEC-INJ-001`: a db/shell call whose template literal interpolates `${`.
static INTERPOLATED_CALL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(?:execSync|exec|execute|query|raw|send)\s*\(\s*`[^`\r\n]*\$\{")
        .expect("valid interpolated call regex")
});

/// `SEC-JWT-003`: a JWT payload decoded without signature verification.
static JWT_DECODE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bjwt\.decode\s*\(").expect("valid jwt decode regex"));

/// `SEC-JWT-003`: an `algorithms` configuration entry.
static JWT_ALGORITHMS_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\balgorithms\b").expect("valid algorithms regex"));

/// `SEC-JWT-003`: a quoted `none` algorithm value.
static QUOTED_NONE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"['"]none['"]"#).expect("valid quoted none regex"));

/// `SEC-XSS-004`: unsanitized HTML sink writes.
static XSS_SINK_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"dangerouslySetInnerHTML|\.innerHTML\s*=|\bdocument\.write\s*\(")
        .expect("valid XSS sink regex")
});

/// `ARCH-SRP-002`: a class declaration line.
static CLASS_DECL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bclass\s+\w+").expect("valid class declaration regex"));

/// `ARCH-SRP-002`: persistence concern group.
static PERSISTENCE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\.query\(|new\s+\w*Repository|knex\(|prisma\.|\.save\(")
        .expect("valid persistence regex")
});

/// `ARCH-SRP-002`: network/presentation concern group.
static NETWORK_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"\.(?:get|post|put|delete)\(\s*['"`/]|\bres\.(?:json|send)\(|\bfetch\(|axios\.|router\."#,
    )
    .expect("valid network regex")
});

/// `ARCH-SRP-002`: UI concern group.
static UI_CONCERN_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"render\(|useState").expect("valid UI concern regex"));

/// `ARCH-DIP-003`: inline construction of a concrete adapter.
static CONCRETE_INSTANTIATION_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"\bnew\s+\w*(?:Repository|Adapter|Client|Gateway|Service|Provider|Connection)\w*\s*\(",
    )
    .expect("valid concrete instantiation regex")
});

/// `ARCH-BND-004`: HTTP boundary files.
static BOUNDARY_FILE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:controller|route|handler|\bapi\b)").expect("valid boundary file regex")
});

/// `ARCH-BND-004`: a route handler responding with a domain entity or model.
static ENTITY_RESPONSE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"res\.(?:json|send)\(\s*\w*(?:Entity|Model|Aggregate)\w*\b")
        .expect("valid entity response regex")
});

/// `EDGE-NPE-001`: nested request-property access.
static REQUEST_PROPERTY_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\breq\.(?:body|query|params)\.\w+\.\w+").expect("valid request property regex")
});

/// `EDGE-NPE-001`: `.find(...)` result dereferenced without `?.`.
static FIND_DOT_ACCESS_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\.find\([^()\r\n]*\)\.(.)?").expect("valid find access regex"));

/// `EDGE-NPE-001`: `.getItem(...)` result dereferenced without `?.`.
static STORAGE_DOT_ACCESS_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\blocalStorage\.getItem\([^)\r\n]*\)\.(.)?").expect("valid storage access regex")
});

/// `EDGE-PRM-002`: a promise `.then(` continuation.
static THEN_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\.then\s*\(").expect("valid then regex"));

/// `EDGE-PRM-002`: any `catch` token.
static CATCH_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bcatch\b").expect("valid catch regex"));

/// `EDGE-PRM-002`: any `try` token.
static TRY_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\btry\b").expect("valid try regex"));

/// `EDGE-PRM-002`: a `fetch(` call.
static FETCH_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bfetch\s*\(").expect("valid fetch regex"));

/// `EDGE-BND-003`: indexing with a variable rather than a numeric literal.
static VARIABLE_INDEX_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b\w+\[[a-z_]\w*\]").expect("valid variable index regex"));

/// `EDGE-TMO-004`: network calls subject to timeouts.
static NETWORK_CALL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\bfetch\s*\(|axios\.(?:get|post|put|delete|patch|head|options|request)\s*\(")
        .expect("valid network call regex")
});

/// `PERF-NPO-001`: iteration constructs.
static LOOP_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\bfor\s*\(|\bfor\s+await\b|\.forEach\s*\(|\bwhile\s*\(").expect("valid loop regex")
});

/// `PERF-NPO-001`: database calls.
static DB_CALL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\.(?:query|findById|findUnique|findMany|findOne|save|insert)\(|prisma\.|knex\(")
        .expect("valid db call regex")
});

/// `PERF-HDL-002`: handle-opening calls.
static HANDLE_OPEN_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:fs\.open|openSync|createConnection|getConnection|createClient)\(")
        .expect("valid handle open regex")
});

/// `PERF-EVT-003`: listener registration.
static ADD_LISTENER_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"addEventListener\s*\(").expect("valid add listener regex"));

/// `PERF-EVT-003`: effect hook invocations.
static EFFECT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"useEffect\s*\(").expect("valid effect regex"));

/// `PERF-EVT-003`: an effect cleanup return.
static CLEANUP_RETURN_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"return\s*\(\s*\)\s*=>").expect("valid cleanup return regex"));

/// Iterate only the chunks of one kind: AST heuristics must never run against
/// whole-file chunks, and file/module heuristics must never run against
/// windows (`CR-SPEC-04 §8`).
fn chunks_of_kind(chunks: &[CodeChunk], kind: ChunkKind) -> impl Iterator<Item = &CodeChunk> {
    chunks.iter().filter(move |chunk| chunk.kind == kind)
}

/// Emit at most one finding per matching line of every chunk of `kind`, in
/// chunk order. `classify` returns the generic description and the matrix
/// confidence for a line; `None` means the line is clean.
fn scan_chunk_lines<F>(
    rule: &RuleSpec,
    chunks: &[CodeChunk],
    kind: ChunkKind,
    mut classify: F,
) -> Vec<Finding>
where
    F: FnMut(&CodeChunk, u32, &str) -> Option<(&'static str, f32)>,
{
    let mut findings = Vec::new();
    for chunk in chunks_of_kind(chunks, kind) {
        for (line, text) in chunk_lines(chunk) {
            if let Some((description, confidence)) = classify(chunk, line, text) {
                findings.push(build_finding(
                    rule,
                    &chunk.file_path,
                    line,
                    line,
                    description,
                    confidence,
                ));
            }
        }
    }
    findings
}

/// Like [`scan_chunk_lines`], but computes a per-chunk `context` once before
/// iterating the chunk's lines, so chunk-wide predicates are never re-scanned
/// for every line (that per-line repetition is quadratic in chunk size).
fn scan_chunk_lines_ctx<C, I, F>(
    rule: &RuleSpec,
    chunks: &[CodeChunk],
    kind: ChunkKind,
    init: I,
    mut classify: F,
) -> Vec<Finding>
where
    I: Fn(&CodeChunk) -> C,
    F: FnMut(&C, &CodeChunk, u32, &str) -> Option<(&'static str, f32)>,
{
    let mut findings = Vec::new();
    for chunk in chunks_of_kind(chunks, kind) {
        let context = init(chunk);
        for (line, text) in chunk_lines(chunk) {
            if let Some((description, confidence)) = classify(&context, chunk, line, text) {
                findings.push(build_finding(
                    rule,
                    &chunk.file_path,
                    line,
                    line,
                    description,
                    confidence,
                ));
            }
        }
    }
    findings
}

/// `true` when a dot-access match from `re` is not followed by `?`.
///
/// The `regex` crate has no look-around, so the heuristic matrix's `(?!\?)`
/// guard is implemented as a post-match check on the capture group: group 1 is
/// the character that follows the access dot, when one exists.
fn dot_access_without_optional_chain(line: &str, re: &Regex) -> bool {
    re.captures_iter(line)
        .any(|captures| match captures.get(1) {
            Some(next) => next.as_str() != "?",
            None => true,
        })
}

/// `SEC-INJ-001`: concatenated SQL strings or interpolated db/shell templates.
fn security_injection(rule: &RuleSpec, chunks: &[CodeChunk]) -> Vec<Finding> {
    scan_chunk_lines(rule, chunks, ChunkKind::AstWindow, |_chunk, _line, text| {
        if SQL_KEYWORD_RE.is_match(text) && SQL_STRING_CONCAT_RE.is_match(text) {
            Some((DESC_INJ_CONCAT, 0.65))
        } else if INTERPOLATED_CALL_RE.is_match(text) {
            Some((DESC_INJ_TEMPLATE, 0.65))
        } else {
            None
        }
    })
}

/// `SEC-JWT-003`: unverified JWT decoding or an accepted `none` algorithm.
fn security_jwt(rule: &RuleSpec, chunks: &[CodeChunk]) -> Vec<Finding> {
    scan_chunk_lines(rule, chunks, ChunkKind::AstWindow, |_chunk, _line, text| {
        if JWT_ALGORITHMS_RE.is_match(text) && QUOTED_NONE_RE.is_match(text) {
            Some((DESC_JWT_NONE, 0.9))
        } else if JWT_DECODE_RE.is_match(text) {
            Some((DESC_JWT_DECODE, 0.75))
        } else {
            None
        }
    })
}

/// `SEC-XSS-004`: direct HTML sink writes.
fn security_xss(rule: &RuleSpec, chunks: &[CodeChunk]) -> Vec<Finding> {
    scan_chunk_lines(rule, chunks, ChunkKind::AstWindow, |_chunk, _line, text| {
        (XSS_SINK_RE.is_match(text) && !is_sanitized(text)).then_some((DESC_XSS, 0.7))
    })
}

/// `SEC-XSS-004`: a sink line routed through a sanitizer is not raw HTML.
fn is_sanitized(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("dompurify") || lower.contains("sanitiz")
}

/// `ARCH-SRP-002`: one oversized class mixing two or more concern groups.
fn architecture_srp(rule: &RuleSpec, chunks: &[CodeChunk]) -> Vec<Finding> {
    let mut findings = Vec::new();
    for chunk in chunks_of_kind(chunks, ChunkKind::AstWindow) {
        if chunk.line_count() < SRP_MIN_LINES {
            continue;
        }
        let concerns = [
            PERSISTENCE_RE.is_match(&chunk.content),
            NETWORK_RE.is_match(&chunk.content),
            UI_CONCERN_RE.is_match(&chunk.content),
        ];
        if concerns.iter().filter(|present| **present).count() < 2 {
            continue;
        }
        if let Some((line, _)) = chunk_lines(chunk).find(|(_, text)| CLASS_DECL_RE.is_match(text)) {
            findings.push(build_finding(
                rule,
                &chunk.file_path,
                line,
                line,
                DESC_SRP,
                0.5,
            ));
        }
    }
    findings
}

/// `ARCH-DIP-003`: concrete adapter construction inside domain/use-case code.
fn architecture_dip(rule: &RuleSpec, chunks: &[CodeChunk]) -> Vec<Finding> {
    scan_chunk_lines(rule, chunks, ChunkKind::AstWindow, |chunk, _line, text| {
        let inner_layer =
            chunk.file_path.contains("domain/") || chunk.file_path.contains("usecase");
        (inner_layer && CONCRETE_INSTANTIATION_RE.is_match(text)).then_some((DESC_DIP, 0.55))
    })
}

/// `ARCH-BND-004`: domain entities serialized straight out of a route handler.
fn architecture_boundary(rule: &RuleSpec, chunks: &[CodeChunk]) -> Vec<Finding> {
    scan_chunk_lines(rule, chunks, ChunkKind::FileModule, |chunk, _line, text| {
        (BOUNDARY_FILE_RE.is_match(&chunk.file_path) && ENTITY_RESPONSE_RE.is_match(text))
            .then_some((DESC_BND, 0.6))
    })
}

/// `EDGE-NPE-001`: property access chained onto a possibly-null value.
fn edge_npe(rule: &RuleSpec, chunks: &[CodeChunk]) -> Vec<Finding> {
    scan_chunk_lines(rule, chunks, ChunkKind::AstWindow, |_chunk, _line, text| {
        let flagged = REQUEST_PROPERTY_RE.is_match(text)
            || dot_access_without_optional_chain(text, &FIND_DOT_ACCESS_RE)
            || dot_access_without_optional_chain(text, &STORAGE_DOT_ACCESS_RE);
        flagged.then_some((DESC_NPE, 0.55))
    })
}

/// `EDGE-PRM-002`: promise chains and fetches without rejection handling.
fn edge_promise(rule: &RuleSpec, chunks: &[CodeChunk]) -> Vec<Finding> {
    scan_chunk_lines_ctx(
        rule,
        chunks,
        ChunkKind::AstWindow,
        |chunk| {
            (
                chunk.content.contains(".catch("),
                CATCH_RE.is_match(&chunk.content) || TRY_RE.is_match(&chunk.content),
            )
        },
        |&(has_catch_call, handled), _chunk, _line, text| {
            if !has_catch_call && THEN_RE.is_match(text) {
                return Some((DESC_PRM_THEN, 0.5));
            }
            (!handled && FETCH_RE.is_match(text)).then_some((DESC_PRM_FETCH, 0.5))
        },
    )
}

/// `EDGE-BND-003`: variable array indexing without length or bounds checks.
fn edge_bounds(rule: &RuleSpec, chunks: &[CodeChunk]) -> Vec<Finding> {
    scan_chunk_lines_ctx(
        rule,
        chunks,
        ChunkKind::AstWindow,
        |chunk| chunk.content.contains(".length") || chunk.content.contains("Array.isArray"),
        |guarded, _chunk, _line, text| {
            (!*guarded && VARIABLE_INDEX_RE.is_match(text)).then_some((DESC_IDX, 0.4))
        },
    )
}

/// `EDGE-TMO-004`: network calls without timeout or abort configuration.
fn edge_timeout(rule: &RuleSpec, chunks: &[CodeChunk]) -> Vec<Finding> {
    scan_chunk_lines_ctx(
        rule,
        chunks,
        ChunkKind::AstWindow,
        |chunk| {
            chunk.content.contains("timeout")
                || chunk.content.contains("AbortController")
                || chunk.content.contains("signal")
                || chunk.content.contains("abort")
        },
        |bounded, _chunk, _line, text| {
            (!*bounded && NETWORK_CALL_RE.is_match(text)).then_some((DESC_TMO, 0.6))
        },
    )
}

/// `PERF-NPO-001`: database calls lexically **inside** an iteration loop.
///
/// Calls on the loop-header line itself (`ids.forEach((id) => db.query(q))`)
/// count as inside; a headerless callback body spread over lines without
/// braces is conservatively not flagged (precision first). The tracker is
/// best-effort: braces inside string literals skew it like in every
/// line-scoped heuristic here.
fn performance_n_plus_one(rule: &RuleSpec, chunks: &[CodeChunk]) -> Vec<Finding> {
    let mut findings = Vec::new();
    for chunk in chunks_of_kind(chunks, ChunkKind::AstWindow) {
        let mut depth: i64 = 0;
        // Brace depth below which each still-open loop body sits: a line is
        // inside that body while the running depth is strictly deeper.
        let mut loop_entries: Vec<i64> = Vec::new();
        for (line, text) in chunk_lines(chunk) {
            let depth_before = depth;
            let loop_header = LOOP_RE.is_match(text);
            let inside = loop_entries.iter().any(|&entry| depth > entry);
            if DB_CALL_RE.is_match(text) && (inside || loop_header) {
                findings.push(build_finding(
                    rule,
                    &chunk.file_path,
                    line,
                    line,
                    DESC_NPO,
                    0.55,
                ));
            }
            depth += brace_delta(text);
            loop_entries.retain(|&entry| depth > entry);
            if loop_header {
                loop_entries.push(depth_before);
            }
        }
    }
    findings
}

/// Net brace-depth change of one source line (string literals included: an
/// unbalanced brace inside a literal skews the best-effort tracker).
fn brace_delta(text: &str) -> i64 {
    text.matches('{').count() as i64 - text.matches('}').count() as i64
}

/// `PERF-HDL-002`: handles opened without a guaranteed release clause.
fn performance_handle(rule: &RuleSpec, chunks: &[CodeChunk]) -> Vec<Finding> {
    scan_chunk_lines_ctx(
        rule,
        chunks,
        ChunkKind::FileModule,
        |chunk| {
            chunk.content.contains(".close(")
                || chunk.content.contains(".end(")
                || chunk.content.contains(".release(")
                || chunk.content.contains("finally")
        },
        |released, _chunk, _line, text| {
            (!*released && HANDLE_OPEN_RE.is_match(text)).then_some((DESC_HDL, 0.6))
        },
    )
}

/// `PERF-EVT-003`: listeners and effects that are never detached.
///
/// A chunk that carries a `removeEventListener` or an effect cleanup return
/// (`return () =>`) is considered detached: both branches of the rule stay
/// silent, because a subscription paired with a cleanup is the pattern the
/// rule asks for.
fn performance_events(rule: &RuleSpec, chunks: &[CodeChunk]) -> Vec<Finding> {
    scan_chunk_lines_ctx(
        rule,
        chunks,
        ChunkKind::AstWindow,
        |chunk| {
            (
                !chunk.content.contains("removeEventListener"),
                CLEANUP_RETURN_RE.is_match(&chunk.content),
            )
        },
        |&(detached, cleaned_up), _chunk, _line, text| {
            if !detached || cleaned_up {
                return None;
            }
            ADD_LISTENER_RE
                .is_match(text)
                .then_some((DESC_EVT_ADD, 0.6))
                .or_else(|| EFFECT_RE.is_match(text).then_some((DESC_EVT_EFFECT, 0.5)))
        },
    )
}

/// Regex-heuristic evaluator for the rules Tier 0 does not own.
#[derive(Debug, Default)]
pub struct MockRuleEvaluator {
    latency_ms: u64,
}

impl MockRuleEvaluator {
    /// Create an evaluator with zero synthetic latency.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create an evaluator that sleeps `latency_ms` per batch, emulating a
    /// cloud round trip for UI animation (10–35 ms band, `CR-SPEC-08 §5`).
    #[must_use]
    pub fn with_latency_ms(latency_ms: u64) -> Self {
        Self { latency_ms }
    }

    /// Evaluate one rule against the chunk batch, returning its findings in
    /// chunk/line order. Unknown ids and rules owned by another engine yield
    /// an empty list.
    fn evaluate_rule(rule: &RuleSpec, chunks: &[CodeChunk]) -> Vec<Finding> {
        match rule.id.as_str() {
            id if TIER0_OWNED_IDS.contains(&id) => Vec::new(),
            id if GIT_CHANGESET_IDS.contains(&id) => Vec::new(),
            "SEC-INJ-001" => security_injection(rule, chunks),
            "SEC-JWT-003" => security_jwt(rule, chunks),
            "SEC-XSS-004" => security_xss(rule, chunks),
            "ARCH-SRP-002" => architecture_srp(rule, chunks),
            "ARCH-DIP-003" => architecture_dip(rule, chunks),
            "ARCH-BND-004" => architecture_boundary(rule, chunks),
            "EDGE-NPE-001" => edge_npe(rule, chunks),
            "EDGE-PRM-002" => edge_promise(rule, chunks),
            "EDGE-BND-003" => edge_bounds(rule, chunks),
            "EDGE-TMO-004" => edge_timeout(rule, chunks),
            "PERF-NPO-001" => performance_n_plus_one(rule, chunks),
            "PERF-HDL-002" => performance_handle(rule, chunks),
            "PERF-EVT-003" => performance_events(rule, chunks),
            _ => Vec::new(),
        }
    }
}

impl SemanticRuleEvaluator for MockRuleEvaluator {
    fn evaluate_batch<'a>(
        &'a self,
        chunks: Vec<CodeChunk>,
        rules: &'a [RuleSpec],
    ) -> BoxFuture<'a, Result<Vec<RuleEvaluationResult>, DomainError>> {
        Box::pin(async move {
            if self.latency_ms > 0 {
                std::thread::sleep(Duration::from_millis(self.latency_ms));
            }
            let results: Vec<RuleEvaluationResult> = rules
                .iter()
                .map(|rule| {
                    let findings = Self::evaluate_rule(rule, &chunks);
                    RuleEvaluationResult::new(
                        rule.id.clone(),
                        EvaluationEngine::MockHeuristic,
                        findings,
                    )
                })
                .collect();
            Ok(results)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::block_on;
    use crate::RuleManifest;
    use coderadar_domain::RuleScope;

    fn embedded_rule(id: &str) -> RuleSpec {
        RuleManifest::embedded()
            .expect("embedded manifest parses")
            .rule_by_id(id)
            .expect("rule id exists in the manifest")
            .clone()
    }

    fn evaluate_one(rule_id: &str, chunks: Vec<CodeChunk>) -> Vec<Finding> {
        let rule = embedded_rule(rule_id);
        let results = block_on(MockRuleEvaluator::new().evaluate_batch(chunks, &[rule]))
            .expect("mock evaluation cannot fail");
        assert_eq!(results.len(), 1);
        results.into_iter().next().expect("one result").findings
    }

    fn assert_single_finding(
        findings: &[Finding],
        rule_id: &str,
        file_path: &str,
        line: u32,
        confidence: f32,
    ) {
        assert_eq!(findings.len(), 1, "expected one finding, got {findings:#?}");
        let finding = &findings[0];
        assert_eq!(finding.rule_id, rule_id);
        assert_eq!(finding.file_path, file_path);
        assert_eq!(finding.line_start, line);
        assert_eq!(finding.line_end, line);
        assert!(
            (finding.confidence - confidence).abs() < f32::EPSILON,
            "confidence {} != {confidence}",
            finding.confidence
        );
    }

    #[test]
    fn sec_inj_flags_concatenated_sql() {
        let chunk = CodeChunk::new(
            "src/db/users.ts",
            "typescript",
            40,
            41,
            "const sql = \"SELECT * FROM users WHERE id = \" + req.query.id;\nreturn db.query(sql);\n",
        );
        let findings = evaluate_one("SEC-INJ-001", vec![chunk]);
        assert_single_finding(&findings, "SEC-INJ-001", "src/db/users.ts", 40, 0.65);
    }

    #[test]
    fn sec_inj_flags_interpolated_template_call() {
        let chunk = CodeChunk::new(
            "src/db/users.ts",
            "typescript",
            12,
            12,
            "db.query(`SELECT * FROM users WHERE id = ${id}`);\n",
        );
        let findings = evaluate_one("SEC-INJ-001", vec![chunk]);
        assert_single_finding(&findings, "SEC-INJ-001", "src/db/users.ts", 12, 0.65);
    }

    #[test]
    fn sec_inj_reports_absolute_lines_across_chunks() {
        let first = CodeChunk::new(
            "src/db/users.ts",
            "typescript",
            40,
            40,
            "const sql = \"SELECT * FROM users WHERE id = \" + id;\n",
        );
        let second = CodeChunk::new(
            "src/db/users.ts",
            "typescript",
            90,
            90,
            "db.query(`SELECT * FROM users WHERE id = ${id}`);\n",
        );
        let findings = evaluate_one("SEC-INJ-001", vec![first, second]);
        assert_eq!(findings.len(), 2);
        assert_eq!(findings[0].line_start, 40);
        assert_eq!(findings[1].line_start, 90);
    }

    #[test]
    fn sec_jwt_flags_unverified_decode() {
        let chunk = CodeChunk::new(
            "src/auth/jwt.ts",
            "typescript",
            7,
            7,
            "const payload = jwt.decode(token);\n",
        );
        let findings = evaluate_one("SEC-JWT-003", vec![chunk]);
        assert_single_finding(&findings, "SEC-JWT-003", "src/auth/jwt.ts", 7, 0.75);
    }

    #[test]
    fn sec_jwt_flags_alg_none() {
        let chunk = CodeChunk::new(
            "src/auth/jwt.ts",
            "typescript",
            9,
            9,
            "jwt.verify(token, secret, { algorithms: [\"none\"] });\n",
        );
        let findings = evaluate_one("SEC-JWT-003", vec![chunk]);
        assert_single_finding(&findings, "SEC-JWT-003", "src/auth/jwt.ts", 9, 0.9);
    }

    #[test]
    fn sec_xss_flags_unsanitized_sink() {
        let chunk = CodeChunk::new(
            "src/ui/Profile.tsx",
            "typescript",
            21,
            22,
            "function Bio({ html }) {\n  return <div dangerouslySetInnerHTML={{ __html: html }} />;\n}\n",
        );
        let findings = evaluate_one("SEC-XSS-004", vec![chunk]);
        assert_single_finding(&findings, "SEC-XSS-004", "src/ui/Profile.tsx", 22, 0.7);
    }

    #[test]
    fn sec_xss_ignores_file_module_chunks() {
        let chunk = CodeChunk::file_module(
            "src/ui/Profile.tsx",
            "typescript",
            1,
            "el.innerHTML = html;\n",
        );
        let findings = evaluate_one("SEC-XSS-004", vec![chunk]);
        assert!(findings.is_empty());
    }

    #[test]
    fn sec_xss_ignores_sanitized_sink() {
        let content = "el.innerHTML = DOMPurify.sanitize(html);\n";
        let chunk = CodeChunk::new("src/ui/Profile.tsx", "typescript", 21, 22, content);
        let findings = evaluate_one("SEC-XSS-004", vec![chunk]);
        assert!(findings.is_empty());
    }

    #[test]
    fn sec_xss_ignores_sanitized_props() {
        let content = "return <div dangerouslySetInnerHTML={{ __html: sanitize(raw) }} />;\n";
        let chunk = CodeChunk::new("src/ui/Profile.tsx", "typescript", 4, 4, content);
        let findings = evaluate_one("SEC-XSS-004", vec![chunk]);
        assert!(findings.is_empty());
    }

    #[test]
    fn arch_srp_flags_oversized_multi_concern_class() {
        let mut lines: Vec<String> = vec![
            "export class OrderService {".to_string(),
            "  async load(id) { return this.db.query(\"SELECT * FROM orders\"); }".to_string(),
            "  async respond(res) { return res.json({ ok: true }); }".to_string(),
        ];
        while lines.len() < 220 {
            lines.push("  // padding".to_string());
        }
        let chunk = CodeChunk::new(
            "src/orders/OrderService.ts",
            "typescript",
            1,
            220,
            lines.join("\n"),
        );
        let findings = evaluate_one("ARCH-SRP-002", vec![chunk]);
        assert_single_finding(
            &findings,
            "ARCH-SRP-002",
            "src/orders/OrderService.ts",
            1,
            0.5,
        );
    }

    #[test]
    fn arch_srp_ignores_small_class() {
        let content = "export class OrderService {\n  async load(id) { return this.db.query(\"SELECT 1\"); }\n  async respond(res) { return res.json({ ok: true }); }\n}\n";
        let chunk = CodeChunk::new("src/orders/OrderService.ts", "typescript", 1, 4, content);
        let findings = evaluate_one("ARCH-SRP-002", vec![chunk]);
        assert!(findings.is_empty());
    }

    #[test]
    fn arch_dip_flags_concrete_instantiation_in_inner_layer() {
        let chunk = CodeChunk::new(
            "src/domain/orders/place_order.ts",
            "typescript",
            15,
            15,
            "const repo = new OrderRepository(db);\n",
        );
        let findings = evaluate_one("ARCH-DIP-003", vec![chunk]);
        assert_single_finding(
            &findings,
            "ARCH-DIP-003",
            "src/domain/orders/place_order.ts",
            15,
            0.55,
        );
    }

    #[test]
    fn arch_dip_ignores_outer_layer_file() {
        let chunk = CodeChunk::new(
            "src/web/handlers/order.ts",
            "typescript",
            15,
            15,
            "const repo = new OrderRepository(db);\n",
        );
        let findings = evaluate_one("ARCH-DIP-003", vec![chunk]);
        assert!(findings.is_empty());
    }

    #[test]
    fn arch_bnd_flags_entity_response_in_controller() {
        let content = "router.get('/user', (req, res) => {\n  res.json(userEntity);\n});\n";
        let chunk = CodeChunk::file_module("src/api/userController.ts", "typescript", 3, content);
        let findings = evaluate_one("ARCH-BND-004", vec![chunk]);
        assert_single_finding(
            &findings,
            "ARCH-BND-004",
            "src/api/userController.ts",
            2,
            0.6,
        );
    }

    #[test]
    fn arch_bnd_ignores_ast_window_chunks() {
        let chunk = CodeChunk::new(
            "src/api/userController.ts",
            "typescript",
            1,
            1,
            "  res.json(userEntity);\n",
        );
        let findings = evaluate_one("ARCH-BND-004", vec![chunk]);
        assert!(findings.is_empty());
    }

    #[test]
    fn edge_npe_flags_unguarded_access() {
        let content = concat!(
            "const name = req.body.user.name;\n",
            "const first = items.find(x => x.id).value;\n",
            "const token = localStorage.getItem(\"k\").token;\n",
        );
        let chunk = CodeChunk::new("src/client/app.ts", "typescript", 5, 7, content);
        let findings = evaluate_one("EDGE-NPE-001", vec![chunk]);
        assert_eq!(findings.len(), 3);
        assert_eq!(findings[0].line_start, 5);
        assert_eq!(findings[1].line_start, 6);
        assert_eq!(findings[2].line_start, 7);
        for finding in &findings {
            assert_eq!(finding.rule_id, "EDGE-NPE-001");
            assert!((finding.confidence - 0.55).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn edge_npe_ignores_optional_chains() {
        let content = concat!(
            "const name = req.body.user?.name;\n",
            "const first = items.find(x => x.id)?.value;\n",
            "const token = localStorage.getItem(\"k\")?.token;\n",
        );
        let chunk = CodeChunk::new("src/client/app.ts", "typescript", 1, 3, content);
        let findings = evaluate_one("EDGE-NPE-001", vec![chunk]);
        assert!(findings.is_empty());
    }

    #[test]
    fn edge_prm_flags_unhandled_promise_chain() {
        let chunk = CodeChunk::new(
            "src/api/load.ts",
            "typescript",
            30,
            30,
            "loadUser(id).then(render);\n",
        );
        let findings = evaluate_one("EDGE-PRM-002", vec![chunk]);
        assert_single_finding(&findings, "EDGE-PRM-002", "src/api/load.ts", 30, 0.5);
    }

    #[test]
    fn edge_prm_flags_unhandled_fetch() {
        let chunk = CodeChunk::new(
            "src/api/load.ts",
            "typescript",
            12,
            12,
            "const response = fetch(url);\n",
        );
        let findings = evaluate_one("EDGE-PRM-002", vec![chunk]);
        assert_single_finding(&findings, "EDGE-PRM-002", "src/api/load.ts", 12, 0.5);
    }

    #[test]
    fn edge_prm_ignores_handled_promises() {
        let content = concat!(
            "loadUser(id).then(render).catch(report);\n",
            "try {\n",
            "  fetch(url);\n",
            "} catch (error) {\n",
            "  report(error);\n",
            "}\n",
        );
        let chunk = CodeChunk::new("src/api/load.ts", "typescript", 1, 6, content);
        let findings = evaluate_one("EDGE-PRM-002", vec![chunk]);
        assert!(findings.is_empty());
    }

    #[test]
    fn edge_bnd_flags_variable_index_without_length_check() {
        let chunk = CodeChunk::new(
            "src/list.ts",
            "typescript",
            8,
            8,
            "const item = items[index];\n",
        );
        let findings = evaluate_one("EDGE-BND-003", vec![chunk]);
        assert_single_finding(&findings, "EDGE-BND-003", "src/list.ts", 8, 0.4);
    }

    #[test]
    fn edge_bnd_ignores_length_checked_index() {
        let content = "if (index < items.length) {\n  const item = items[index];\n}\n";
        let chunk = CodeChunk::new("src/list.ts", "typescript", 1, 3, content);
        let findings = evaluate_one("EDGE-BND-003", vec![chunk]);
        assert!(findings.is_empty());
    }

    #[test]
    fn edge_tmo_flags_call_without_timeout() {
        let chunk = CodeChunk::new(
            "src/api/client.ts",
            "typescript",
            44,
            44,
            "const response = await fetch(endpoint);\n",
        );
        let findings = evaluate_one("EDGE-TMO-004", vec![chunk]);
        assert_single_finding(&findings, "EDGE-TMO-004", "src/api/client.ts", 44, 0.6);
    }

    #[test]
    fn edge_tmo_ignores_abort_configuration() {
        let content = concat!(
            "const controller = new AbortController();\n",
            "fetch(endpoint, { signal: controller.signal });\n",
        );
        let chunk = CodeChunk::new("src/api/client.ts", "typescript", 1, 2, content);
        let findings = evaluate_one("EDGE-TMO-004", vec![chunk]);
        assert!(findings.is_empty());
    }

    #[test]
    fn perf_npo_flags_db_call_inside_loop() {
        let content = "for (const id of ids) {\n  const row = await db.query(id);\n}\n";
        let chunk = CodeChunk::new("src/repo.ts", "typescript", 3, 5, content);
        let findings = evaluate_one("PERF-NPO-001", vec![chunk]);
        assert_single_finding(&findings, "PERF-NPO-001", "src/repo.ts", 4, 0.55);
    }

    #[test]
    fn perf_npo_ignores_db_call_outside_loop() {
        let chunk = CodeChunk::new(
            "src/repo.ts",
            "typescript",
            9,
            9,
            "const row = await db.query(id);\n",
        );
        let findings = evaluate_one("PERF-NPO-001", vec![chunk]);
        assert!(findings.is_empty());
    }

    #[test]
    fn perf_npo_ignores_db_call_after_the_loop() {
        let content = concat!(
            "for (const id of ids) {\n",
            "  total += id.length;\n",
            "}\n",
            "const first = await db.query(\"select\");\n",
        );
        let chunk = CodeChunk::new("src/repo.ts", "typescript", 1, 4, content);
        let findings = evaluate_one("PERF-NPO-001", vec![chunk]);
        assert!(findings.is_empty());
    }

    #[test]
    fn perf_npo_flags_db_call_on_the_loop_header_line() {
        let chunk = CodeChunk::new(
            "src/repo.ts",
            "typescript",
            7,
            7,
            "ids.forEach((id) => db.query(q));\n",
        );
        let findings = evaluate_one("PERF-NPO-001", vec![chunk]);
        assert_single_finding(&findings, "PERF-NPO-001", "src/repo.ts", 7, 0.55);
    }

    #[test]
    fn perf_hdl_flags_unreleased_handle() {
        let content = "const fd = fs.open(path, \"r\");\nconst data = readSync(fd);\n";
        let chunk = CodeChunk::file_module("src/io/files.ts", "typescript", 2, content);
        let findings = evaluate_one("PERF-HDL-002", vec![chunk]);
        assert_single_finding(&findings, "PERF-HDL-002", "src/io/files.ts", 1, 0.6);
    }

    #[test]
    fn perf_hdl_ignores_released_handle() {
        let content =
            "const fd = fs.open(path, \"r\");\ntry {\n  readSync(fd);\n} finally {\n  fs.close(fd);\n}\n";
        let chunk = CodeChunk::file_module("src/io/files.ts", "typescript", 6, content);
        let findings = evaluate_one("PERF-HDL-002", vec![chunk]);
        assert!(findings.is_empty());
    }

    #[test]
    fn perf_hdl_ignores_ast_window_chunks() {
        let chunk = CodeChunk::new(
            "src/io/files.ts",
            "typescript",
            1,
            1,
            "const fd = fs.open(path, \"r\");\n",
        );
        let findings = evaluate_one("PERF-HDL-002", vec![chunk]);
        assert!(findings.is_empty());
    }

    #[test]
    fn perf_evt_flags_listener_without_cleanup() {
        let chunk = CodeChunk::new(
            "src/ui/Panel.tsx",
            "typescript",
            14,
            14,
            "window.addEventListener(\"resize\", onResize);\n",
        );
        let findings = evaluate_one("PERF-EVT-003", vec![chunk]);
        assert_single_finding(&findings, "PERF-EVT-003", "src/ui/Panel.tsx", 14, 0.6);
    }

    #[test]
    fn perf_evt_flags_effect_without_cleanup() {
        let content = "useEffect(() => {\n  poll();\n}, []);\n";
        let chunk = CodeChunk::new("src/ui/Panel.tsx", "typescript", 20, 22, content);
        let findings = evaluate_one("PERF-EVT-003", vec![chunk]);
        assert_single_finding(&findings, "PERF-EVT-003", "src/ui/Panel.tsx", 20, 0.5);
    }

    #[test]
    fn perf_evt_ignores_cleaned_up_effects() {
        let content = concat!(
            "useEffect(() => {\n",
            "  window.addEventListener(\"resize\", onResize);\n",
            "  return () => window.removeEventListener(\"resize\", onResize);\n",
            "}, []);\n",
        );
        let chunk = CodeChunk::new("src/ui/Panel.tsx", "typescript", 1, 4, content);
        let findings = evaluate_one("PERF-EVT-003", vec![chunk]);
        assert!(findings.is_empty());
    }

    #[test]
    fn perf_evt_ignores_listener_with_cleanup_return() {
        let content = concat!(
            "useEffect(() => {\n",
            "  window.addEventListener(\"resize\", onResize);\n",
            "  return () => element.release();\n",
            "}, []);\n",
        );
        assert!(
            CLEANUP_RETURN_RE.is_match(content),
            "the fixture must carry the cleanup return"
        );
        let chunk = CodeChunk::new("src/ui/Panel.tsx", "typescript", 1, 4, content);
        let findings = evaluate_one("PERF-EVT-003", vec![chunk]);
        assert!(findings.is_empty(), "{:#?}", findings);
    }

    #[test]
    fn tier0_owned_rules_return_empty_results() {
        let chunk = CodeChunk::new(
            "src/db/users.ts",
            "typescript",
            1,
            2,
            "const sql = \"SELECT * FROM users\" + userId;\nconst payload = jwt.decode(token);\n",
        );
        let rules: Vec<RuleSpec> = ["SEC-TOK-002", "SEC-TLS-005", "ARCH-DEP-001"]
            .iter()
            .map(|id| embedded_rule(id))
            .collect();
        let results = block_on(MockRuleEvaluator::new().evaluate_batch(vec![chunk], &rules))
            .expect("mock evaluation cannot fail");
        assert_eq!(results.len(), 3);
        for (result, rule) in results.iter().zip(&rules) {
            assert_eq!(result.rule_id, rule.id);
            assert_eq!(result.engine, EvaluationEngine::MockHeuristic);
            assert!(result.findings.is_empty());
        }
    }

    #[test]
    fn git_changeset_rules_return_empty_results() {
        let chunk = CodeChunk::new(
            "src/db/users.ts",
            "typescript",
            1,
            1,
            "const sql = \"SELECT * FROM users\" + userId;\n",
        );
        let rules: Vec<RuleSpec> = ["PR-SCP-001", "PR-BRK-002", "PR-TST-003"]
            .iter()
            .map(|id| embedded_rule(id))
            .collect();
        let results = block_on(MockRuleEvaluator::new().evaluate_batch(vec![chunk], &rules))
            .expect("mock evaluation cannot fail");
        assert_eq!(results.len(), 3);
        for result in &results {
            assert_eq!(result.engine, EvaluationEngine::MockHeuristic);
            assert!(result.findings.is_empty());
        }
    }

    #[test]
    fn unknown_rule_returns_empty_result() {
        let mut rule = embedded_rule("SEC-INJ-001");
        rule.id = "XX-UNKNOWN-999".to_string();
        let chunk = CodeChunk::new(
            "src/db/users.ts",
            "typescript",
            1,
            1,
            "db.query(`SELECT * FROM users WHERE id = ${id}`);\n",
        );
        let results = block_on(MockRuleEvaluator::new().evaluate_batch(vec![chunk], &[rule]))
            .expect("mock evaluation cannot fail");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].rule_id, "XX-UNKNOWN-999");
        assert!(results[0].findings.is_empty());
    }

    #[test]
    fn clean_chunk_yields_no_findings_for_ast_rules() {
        let content = concat!(
            "export function add(a: number, b: number): number {\n",
            "  return a + b;\n",
            "}\n",
        );
        let chunk = CodeChunk::new("src/math.ts", "typescript", 1, 3, content);
        let manifest = RuleManifest::embedded().expect("embedded manifest parses");
        let ast_rules: Vec<RuleSpec> = manifest
            .rules
            .iter()
            .filter(|rule| rule.scope == RuleScope::AstChunk)
            .cloned()
            .collect();
        assert!(!ast_rules.is_empty());
        let results = block_on(MockRuleEvaluator::new().evaluate_batch(vec![chunk], &ast_rules))
            .expect("mock evaluation cannot fail");
        assert_eq!(results.len(), ast_rules.len());
        for result in &results {
            assert!(
                result.findings.is_empty(),
                "{} should be clean",
                result.rule_id
            );
        }
    }

    #[test]
    fn results_follow_input_rule_order() {
        let mut unknown = embedded_rule("SEC-INJ-001");
        unknown.id = "XX-UNKNOWN-999".to_string();
        let rules = vec![
            embedded_rule("PERF-EVT-003"),
            embedded_rule("SEC-INJ-001"),
            unknown,
            embedded_rule("PR-TST-003"),
        ];
        let chunk = CodeChunk::new(
            "src/db/users.ts",
            "typescript",
            1,
            1,
            "db.query(`SELECT * FROM users WHERE id = ${id}`);\n",
        );
        let results = block_on(MockRuleEvaluator::new().evaluate_batch(vec![chunk], &rules))
            .expect("mock evaluation cannot fail");
        assert_eq!(results.len(), rules.len());
        for (result, rule) in results.iter().zip(&rules) {
            assert_eq!(result.rule_id, rule.id);
            assert_eq!(result.engine, EvaluationEngine::MockHeuristic);
        }
        assert!(results[0].findings.is_empty());
        assert_eq!(results[1].findings.len(), 1);
        assert!(results[2].findings.is_empty());
        assert!(results[3].findings.is_empty());
    }
}
