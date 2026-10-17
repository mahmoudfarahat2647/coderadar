//! Which rule ids each engine owns (`CR-SPEC-03 §2–§3`).
//!
//! Both the Mock heuristic engine and the Jev Tier 1 adapter must cover the
//! **same** semantic rule set so the two are drop-in interchangeable
//! (`CR-SPEC-02 §5`). Keeping the ownership lists in one place stops the two
//! engines from silently drifting apart.

use coderadar_domain::{EvaluationEngine, RuleScope, RuleSpec};

/// Rule ids owned by the Tier 0 static engine (`crate::tier0`): they are
/// evaluated locally and never sent to Jev.
pub const TIER0_OWNED_IDS: [&str; 3] = ["ARCH-DEP-001", "SEC-TOK-002", "SEC-TLS-005"];

/// `git_changeset`-scoped rules; the Git changeset engine arrives in Phase 8,
/// so both Mock and Jev return them empty for now.
pub const GIT_CHANGESET_IDS: [&str; 3] = ["PR-SCP-001", "PR-BRK-002", "PR-TST-003"];

/// `true` when the semantic (Mock/Jev) engines own `rule`.
///
/// A rule is semantic unless it is one of the deterministic Tier 0 rules, a
/// deferred Git-changeset rule, or a rule that declares a non-Tier-1 engine in
/// the manifest. `git_changeset` scope is also excluded defensively.
pub fn is_semantic_rule(rule: &RuleSpec) -> bool {
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
