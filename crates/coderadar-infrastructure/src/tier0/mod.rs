//! Tier 0 — instant local static analysis (`CR-SPEC-03 §3`).
//!
//! Implements the three deterministic rules that never leave the machine:
//!
//! * `ARCH-DEP-001` — Dependency Rule violation (imports vs layer map);
//! * `SEC-TOK-002` — hardcoded secrets & API keys (signature + entropy);
//! * `SEC-TLS-005` — permissive TLS / plaintext protocol usage.
//!
//! All three are evaluated in Rust in well under 0.2 ms per chunk. The
//! evaluator implements the Layer 2 [`SemanticRuleEvaluator`] port so it is
//! interchangeable with the Mock and Jev engines.
//!
//! Every submodule receives the full chunk list and filters by
//! [`coderadar_domain::ChunkKind`] itself: the file/module-scope rules
//! (`ARCH-DEP-001`, `SEC-TLS-005`) need whole-file context and read only
//! [`coderadar_domain::ChunkKind::FileModule`] chunks, while the chunk-scope
//! secrets rule reads only [`coderadar_domain::ChunkKind::AstWindow`] windows.

use std::sync::Arc;

use coderadar_domain::{CodeChunk, DomainError, EvaluationEngine, RuleEvaluationResult, RuleSpec};
use coderadar_usecases::{BoxFuture, SemanticRuleEvaluator};

use crate::layer_map::LayerMap;

mod dependency_rule;
mod secrets;
mod tls;

/// Rule id of the Dependency Rule violation check.
pub const ARCH_DEP_001: &str = "ARCH-DEP-001";
/// Rule id of the hardcoded-secrets check.
pub const SEC_TOK_002: &str = "SEC-TOK-002";
/// Rule id of the permissive-TLS check.
pub const SEC_TLS_005: &str = "SEC-TLS-005";

/// Deterministic Tier 0 static analyser.
pub struct Tier0RuleEvaluator {
    layer_map: Arc<LayerMap>,
}

impl Tier0RuleEvaluator {
    /// Create the evaluator with the scan's layer map.
    pub fn new(layer_map: Arc<LayerMap>) -> Self {
        Self { layer_map }
    }

    /// `true` when this evaluator owns the given rule id.
    pub fn handles_rule(rule_id: &str) -> bool {
        matches!(rule_id, ARCH_DEP_001 | SEC_TOK_002 | SEC_TLS_005)
    }
}

impl SemanticRuleEvaluator for Tier0RuleEvaluator {
    /// Evaluate every requested rule in order, one result per rule.
    ///
    /// Owned rule ids dispatch to their dedicated checker; any other id yields
    /// an empty result so a mis-routed rule degrades to "clean" instead of
    /// failing the batch. Each submodule receives all chunks and applies its
    /// own [`coderadar_domain::ChunkKind`] filter because it knows which
    /// context its rule needs.
    fn evaluate_batch<'a>(
        &'a self,
        chunks: Vec<CodeChunk>,
        rules: &'a [RuleSpec],
    ) -> BoxFuture<'a, Result<Vec<RuleEvaluationResult>, DomainError>> {
        Box::pin(async move {
            let results = rules
                .iter()
                .map(|rule| {
                    let findings = match rule.id.as_str() {
                        ARCH_DEP_001 => dependency_rule::evaluate(rule, &chunks, &self.layer_map),
                        SEC_TOK_002 => secrets::evaluate(rule, &chunks),
                        SEC_TLS_005 => tls::evaluate(rule, &chunks),
                        _ => Vec::new(),
                    };
                    RuleEvaluationResult::new(
                        rule.id.clone(),
                        EvaluationEngine::Tier0Static,
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
    use std::sync::Arc;

    use coderadar_domain::{
        CodeChunk, EvaluationEngine, FindingCategory, RuleScope, RuleSpec, Severity,
    };
    use coderadar_usecases::SemanticRuleEvaluator;

    use super::{LayerMap, Tier0RuleEvaluator, ARCH_DEP_001, SEC_TOK_002};
    use crate::executor::block_on;

    fn rule(id: &str) -> RuleSpec {
        RuleSpec {
            id: id.to_string(),
            title: format!("rule {id}"),
            category: FindingCategory::Security,
            severity: Severity::High,
            cwe: Vec::new(),
            scope: RuleScope::AstChunk,
            engine: EvaluationEngine::Tier0Static,
            assertion: "test".to_string(),
        }
    }

    #[test]
    fn evaluate_batch_returns_one_result_per_rule_in_order() {
        let evaluator = Tier0RuleEvaluator::new(Arc::new(LayerMap::fallback()));
        let rules = [rule(SEC_TOK_002), rule("SEC-JWT-003"), rule(ARCH_DEP_001)];
        let chunk = CodeChunk::new(
            "src/config.ts",
            "typescript",
            1,
            1,
            r#"const token = "ghp_1234567890abcdefghijABCD";"#,
        );

        let results = block_on(evaluator.evaluate_batch(vec![chunk], &rules)).unwrap();

        assert_eq!(results.len(), 3);
        assert_eq!(results[0].rule_id, SEC_TOK_002);
        assert_eq!(results[0].engine, EvaluationEngine::Tier0Static);
        assert_eq!(
            results[0].findings.len(),
            1,
            "secrets dispatch produces a finding"
        );
        assert_eq!(results[1].rule_id, "SEC-JWT-003");
        assert!(results[1].is_clean(), "unowned ids degrade to clean");
        assert_eq!(results[2].rule_id, ARCH_DEP_001);
        assert!(results[2].is_clean(), "no file-module chunks were supplied");
    }

    #[test]
    fn handles_rule_knows_its_three_rules() {
        assert!(Tier0RuleEvaluator::handles_rule(ARCH_DEP_001));
        assert!(Tier0RuleEvaluator::handles_rule(super::SEC_TLS_005));
        assert!(!Tier0RuleEvaluator::handles_rule("SEC-JWT-003"));
    }
}
