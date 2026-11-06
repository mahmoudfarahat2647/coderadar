//! Rule engine router — one [`SemanticRuleEvaluator`] over both engines.
//!
//! Splits batches between deterministic checks, shared local module heuristics,
//! and the selected semantic engine, then merges results in caller order.

use std::sync::Arc;

use coderadar_domain::{CodeChunk, DomainError, RuleEvaluationResult, RuleSpec};
use coderadar_usecases::{BoxFuture, SemanticRuleEvaluator};

use crate::engine_rules::LOCAL_HEURISTIC_IDS;
use crate::layer_map::LayerMap;
use crate::mock::MockRuleEvaluator;
use crate::tier0::Tier0RuleEvaluator;

/// Routes rules to Tier 0 or the semantic engine and merges the results.
pub struct RuleEngineRouter {
    tier0: Tier0RuleEvaluator,
    semantic: Arc<dyn SemanticRuleEvaluator>,
}

impl RuleEngineRouter {
    /// Validate rule ownership, scope and declared engine before starting a scan.
    ///
    /// # Errors
    /// Returns an invariant error for unknown, duplicate, or misconfigured rules.
    pub fn validate_ownership(rules: &[RuleSpec]) -> Result<(), DomainError> {
        crate::engine_rules::validate_ownership(rules)
    }

    /// Create the router with the default Mock semantic engine.
    pub fn new(layer_map: Arc<LayerMap>) -> Self {
        Self {
            tier0: Tier0RuleEvaluator::new(layer_map),
            semantic: Arc::new(MockRuleEvaluator::new()),
        }
    }

    /// Replace the semantic engine (Mock, live Jev, or the fallback decorator).
    #[must_use]
    pub fn with_semantic(mut self, semantic: Arc<dyn SemanticRuleEvaluator>) -> Self {
        self.semantic = semantic;
        self
    }

    /// Replace the semantic engine with a Mock evaluator (latency emulation).
    #[must_use]
    pub fn with_mock(self, mock: MockRuleEvaluator) -> Self {
        self.with_semantic(Arc::new(mock))
    }

    /// Indices of the rules owned by the Tier 0 engine.
    fn tier0_indices(rules: &[RuleSpec]) -> Vec<usize> {
        rules
            .iter()
            .enumerate()
            .filter(|(_, rule)| Tier0RuleEvaluator::handles_rule(&rule.id))
            .map(|(index, _)| index)
            .collect()
    }
}

/// Clone the subset of `rules` selected by `indices`, in input order.
fn subset_of(rules: &[RuleSpec], indices: &[usize]) -> Vec<RuleSpec> {
    indices.iter().map(|&index| rules[index].clone()).collect()
}

/// Scatter one engine's results back into the caller's per-rule slots.
///
/// Results are matched to slots by rule id, so an engine that reorders (or
/// duplicates) results still produces correct per-rule output.
fn scatter_results(
    slots: &mut [Option<RuleEvaluationResult>],
    rules: &[RuleSpec],
    indices: &[usize],
    results: Vec<RuleEvaluationResult>,
    engine: &str,
) -> Result<(), DomainError> {
    if results.len() != indices.len() {
        return Err(DomainError::invariant(format!(
            "{engine} engine returned {} results for {} rules",
            results.len(),
            indices.len()
        )));
    }
    let mut remaining: Vec<Option<RuleEvaluationResult>> = results.into_iter().map(Some).collect();
    for &index in indices {
        let position = remaining.iter().position(|slot| {
            slot.as_ref()
                .is_some_and(|result| result.rule_id == rules[index].id)
        });
        let Some(position) = position else {
            return Err(DomainError::invariant(format!(
                "{engine} engine returned no result for rule `{}`",
                rules[index].id
            )));
        };
        slots[index] = remaining[position].take();
    }
    Ok(())
}

impl SemanticRuleEvaluator for RuleEngineRouter {
    /// Evaluate `rules` against `chunks`, returning exactly one result per
    /// input rule in input order.
    ///
    /// Only the engines with a non-empty subset are invoked, and both see the
    /// same chunks. An engine that returns the wrong number of results — or a
    /// result unrelated to its subset — aborts the batch with
    /// [`DomainError::Invariant`] instead of silently misaligning rules.
    fn evaluate_batch<'a>(
        &'a self,
        chunks: Vec<CodeChunk>,
        rules: &'a [RuleSpec],
    ) -> BoxFuture<'a, Result<Vec<RuleEvaluationResult>, DomainError>> {
        Box::pin(async move {
            Self::validate_ownership(rules)?;
            let mut slots: Vec<Option<RuleEvaluationResult>> =
                (0..rules.len()).map(|_| None).collect();

            let tier0_indices = RuleEngineRouter::tier0_indices(rules);
            if !tier0_indices.is_empty() {
                let subset = subset_of(rules, &tier0_indices);
                let results = self.tier0.evaluate_batch(chunks.clone(), &subset).await?;
                scatter_results(&mut slots, rules, &tier0_indices, results, "tier0")?;
            }

            let local_indices: Vec<usize> = (0..rules.len())
                .filter(|&index| LOCAL_HEURISTIC_IDS.contains(&rules[index].id.as_str()))
                .collect();
            if !local_indices.is_empty() {
                let subset = subset_of(rules, &local_indices);
                let results = MockRuleEvaluator::new()
                    .evaluate_batch(chunks.clone(), &subset)
                    .await?;
                scatter_results(
                    &mut slots,
                    rules,
                    &local_indices,
                    results,
                    "local heuristic",
                )?;
            }

            let semantic_indices: Vec<usize> = (0..rules.len())
                .filter(|index| !tier0_indices.contains(index) && !local_indices.contains(index))
                .collect();
            if !semantic_indices.is_empty() {
                let subset = subset_of(rules, &semantic_indices);
                let results = self.semantic.evaluate_batch(chunks, &subset).await?;
                scatter_results(&mut slots, rules, &semantic_indices, results, "semantic")?;
            }

            slots
                .into_iter()
                .enumerate()
                .map(|(index, slot)| {
                    slot.ok_or_else(|| {
                        DomainError::invariant(format!(
                            "no evaluation result for rule `{}`",
                            rules[index].id
                        ))
                    })
                })
                .collect()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::RuleEngineRouter;
    use crate::executor::block_on;
    use crate::layer_map::LayerMap;
    use crate::rules_manifest::RuleManifest;
    use coderadar_domain::{CodeChunk, EvaluationEngine, RuleEvaluationResult, RuleSpec};
    use coderadar_usecases::SemanticRuleEvaluator;
    use std::sync::Arc;

    fn embedded_rule(id: &str) -> RuleSpec {
        RuleManifest::embedded()
            .expect("embedded manifest parses")
            .rule_by_id(id)
            .expect("rule id exists in the manifest")
            .clone()
    }

    fn evaluate(
        router: &RuleEngineRouter,
        chunk: CodeChunk,
        rules: &[RuleSpec],
    ) -> Vec<RuleEvaluationResult> {
        block_on(router.evaluate_batch(vec![chunk], rules)).expect("router evaluation cannot fail")
    }

    fn violating_chunk() -> CodeChunk {
        CodeChunk::new(
            "src/config.ts",
            "typescript",
            1,
            2,
            concat!(
                "const token = \"ghp_1234567890abcdefghijABCD\";\n",
                "const payload = jwt.decode(token);\n",
            ),
        )
    }

    #[test]
    fn mixed_batch_routes_each_rule_to_its_engine_in_input_order() {
        let router = RuleEngineRouter::new(Arc::new(LayerMap::fallback()));
        let rules = [
            embedded_rule("SEC-TOK-002"),
            embedded_rule("SEC-JWT-003"),
            embedded_rule("PR-SCP-001"),
        ];

        let results = evaluate(&router, violating_chunk(), &rules);

        assert_eq!(results.len(), 3, "one result per input rule");
        for (result, rule) in results.iter().zip(&rules) {
            assert_eq!(result.rule_id, rule.id, "results follow input order");
        }

        assert_eq!(results[0].engine, EvaluationEngine::Tier0Static);
        assert_eq!(results[0].findings.len(), 1, "tier0 rule finds the secret");
        assert_eq!(results[0].findings[0].line_start, 1);

        assert_eq!(results[1].engine, EvaluationEngine::MockHeuristic);
        assert_eq!(results[1].findings.len(), 1, "mock rule finds jwt.decode");
        assert_eq!(results[1].findings[0].line_start, 2);

        assert_eq!(results[2].engine, EvaluationEngine::MockHeuristic);
        assert!(
            results[2].findings.is_empty(),
            "git_changeset rules stay empty"
        );
    }

    #[test]
    fn tier0_only_batches_never_consult_the_mock_engine() {
        let router = RuleEngineRouter::new(Arc::new(LayerMap::fallback()));
        let rules = [embedded_rule("SEC-TOK-002")];

        let results = evaluate(&router, violating_chunk(), &rules);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].engine, EvaluationEngine::Tier0Static);
        assert_eq!(results[0].findings.len(), 1);
    }

    #[test]
    fn mock_only_batches_never_consult_the_tier0_engine() {
        let router = RuleEngineRouter::new(Arc::new(LayerMap::fallback()));
        let rules = [embedded_rule("SEC-JWT-003")];

        let results = evaluate(&router, violating_chunk(), &rules);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].engine, EvaluationEngine::MockHeuristic);
        assert_eq!(results[0].findings.len(), 1);
    }

    #[test]
    fn empty_rule_list_yields_empty_results() {
        let router = RuleEngineRouter::new(Arc::new(LayerMap::fallback()));

        let results =
            block_on(router.evaluate_batch(Vec::new(), &[])).expect("empty batch succeeds");

        assert!(results.is_empty());
    }
}
