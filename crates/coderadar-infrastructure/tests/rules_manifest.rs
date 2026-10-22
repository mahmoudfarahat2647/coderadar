use coderadar_domain::{EvaluationEngine, FindingCategory, RuleScope, Severity};
use coderadar_infrastructure::RuleManifest;

const EXPECTED_RULE_COUNT: usize = 19;

#[test]
fn embedded_manifest_loads_with_nineteen_rules() {
    let manifest = RuleManifest::embedded().expect("embedded manifest parses");
    assert_eq!(manifest.len(), EXPECTED_RULE_COUNT);
    assert_eq!(manifest.declared_total_rules, EXPECTED_RULE_COUNT);
    assert_eq!(manifest.ruleset_version, "2026.8.0-pr");
}

#[test]
fn every_pillar_is_represented() {
    let manifest = RuleManifest::embedded().unwrap();
    for category in FindingCategory::all() {
        assert!(
            manifest.rules.iter().any(|rule| rule.category == category),
            "no rules for pillar {category}"
        );
    }
}

#[test]
fn tier_zero_rules_from_phase_two_exist() {
    let manifest = RuleManifest::embedded().unwrap();
    for id in ["ARCH-DEP-001", "SEC-TOK-002", "SEC-TLS-005"] {
        let rule = manifest.rule_by_id(id).expect("tier 0 rule present");
        assert_eq!(rule.engine, EvaluationEngine::Tier0Static);
    }
}

#[test]
fn file_module_rules_run_on_the_tier_zero_engine() {
    // CR-SPEC-04 §8: File/Module-scope rules execute on Tier 0
    // (Tree-sitter + layer map), never on the semantic tier.
    let manifest = RuleManifest::embedded().unwrap();
    for id in ["ARCH-DEP-001", "ARCH-BND-004", "PERF-HDL-002"] {
        let rule = manifest.rule_by_id(id).expect("file/module rule present");
        assert_eq!(rule.scope, RuleScope::FileModule, "{id} scope");
        assert_eq!(rule.engine, EvaluationEngine::Tier0Static, "{id} engine");
    }
}

#[test]
fn critical_rules_are_rated_critical() {
    let manifest = RuleManifest::embedded().unwrap();
    for id in ["SEC-INJ-001", "SEC-TOK-002", "SEC-JWT-003"] {
        let rule = manifest.rule_by_id(id).unwrap();
        assert_eq!(rule.severity, Severity::Critical);
        assert!(rule.primary_cwe().is_some());
    }
}

#[test]
fn manifest_on_disk_matches_embedded_copy() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../rules/manifest.json");
    let on_disk = RuleManifest::load_from_path(path).expect("on-disk manifest parses");
    let embedded = RuleManifest::embedded().unwrap();
    assert_eq!(on_disk, embedded);
}

#[test]
fn manifest_rejects_declared_count_mismatch() {
    let broken = r#"{
        "ruleset_version": "2026.8.0-pr",
        "generated_at": "2026-10-16T21:40:00Z",
        "total_rules": 5,
        "categories": ["security"],
        "signature": "ed25519:x",
        "rules": [{
            "id": "SEC-INJ-001",
            "title": "SQL / Command Injection",
            "category": "security",
            "severity": "CRITICAL",
            "cwe": ["CWE-89"],
            "scope": "ast_chunk",
            "engine": "tier1",
            "assertion": "test"
        }]
    }"#;
    assert!(RuleManifest::from_json(broken).is_err());
}
