//! Which rule ids each engine owns (`CR-SPEC-03 §2–§3`).
//!
//! Local file/module heuristics are routed independently of the interchangeable
//! Mock and Jev semantic engines (`CR-SPEC-17 §4`).

use coderadar_domain::{DomainError, EvaluationEngine, RuleScope, RuleSpec};

/// Rule ids owned by the Tier 0 static engine (`crate::tier0`): they are
/// evaluated locally and never sent to Jev.
pub const TIER0_OWNED_IDS: [&str; 3] = ["ARCH-DEP-001", "SEC-TOK-002", "SEC-TLS-005"];

/// `git_changeset`-scoped rules; the Git changeset engine arrives in Phase 8,
/// so both Mock and Jev return them empty for now.
pub const GIT_CHANGESET_IDS: [&str; 3] = ["PR-SCP-001", "PR-BRK-002", "PR-TST-003"];

/// File/module rules evaluated by the shared local heuristic implementation.
pub(crate) const LOCAL_HEURISTIC_IDS: [&str; 2] = ["ARCH-BND-004", "PERF-HDL-002"];

pub(crate) fn validate_ownership(rules: &[RuleSpec]) -> Result<(), DomainError> {
    let mut seen = std::collections::HashSet::new();
    for rule in rules {
        let expected = match rule.id.as_str() {
            "SEC-TOK-002" => Some((RuleScope::AstChunk, EvaluationEngine::Tier0Static)),
            "ARCH-DEP-001" | "SEC-TLS-005" | "ARCH-BND-004" | "PERF-HDL-002" => {
                Some((RuleScope::FileModule, EvaluationEngine::Tier0Static))
            }
            "PR-SCP-001" | "PR-BRK-002" | "PR-TST-003" => {
                Some((RuleScope::GitChangeset, EvaluationEngine::Tier0Static))
            }
            "SEC-INJ-001" | "SEC-JWT-003" | "SEC-XSS-004" | "ARCH-SRP-002" | "ARCH-DIP-003"
            | "EDGE-NPE-001" | "EDGE-PRM-002" | "EDGE-BND-003" | "EDGE-TMO-004"
            | "PERF-NPO-001" | "PERF-EVT-003" => {
                Some((RuleScope::AstChunk, EvaluationEngine::Tier1Semantic))
            }
            _ => None,
        };
        if !seen.insert(rule.id.as_str()) || expected != Some((rule.scope, rule.engine)) {
            return Err(DomainError::invariant(format!(
                "invalid or duplicate rule ownership for `{}`",
                rule.id
            )));
        }
    }
    Ok(())
}

/// `true` when the semantic (Mock/Jev) engines own `rule`.
///
/// Local heuristic rules, deterministic Tier 0 rules, and Git-changeset rules
/// are excluded, as are rules declaring a static engine or changeset scope.
pub fn is_semantic_rule(rule: &RuleSpec) -> bool {
    if LOCAL_HEURISTIC_IDS.contains(&rule.id.as_str()) {
        return false;
    }
    if TIER0_OWNED_IDS.contains(&rule.id.as_str()) {
        return false;
    }
    if GIT_CHANGESET_IDS.contains(&rule.id.as_str()) {
        return false;
    }
    if rule.engine == EvaluationEngine::Tier0Static {
        return false;
    }
    rule.scope != RuleScope::GitChangeset
}
