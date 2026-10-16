//! Loads and validates `rules/manifest.json`.
//!
//! This is a Layer 4 (frameworks & drivers) concern: reading a file and
//! deserializing JSON are outer-circle details. The manifest is parsed into
//! private DTOs and only then translated into the pure domain [`RuleSpec`], so
//! no serialization attribute ever leaks into the domain core.

use std::path::Path;

use coderadar_domain::{
    DomainError, EvaluationEngine, FindingCategory, RuleScope, RuleSpec, Severity,
};
use serde::Deserialize;

/// The manifest shipped inside the binary, so the ruleset is always available
/// even offline and without a filesystem read.
const EMBEDDED_MANIFEST: &str = include_str!("../../../rules/manifest.json");

#[derive(Debug, Deserialize)]
struct ManifestDto {
    ruleset_version: String,
    generated_at: String,
    total_rules: usize,
    categories: Vec<String>,
    signature: String,
    rules: Vec<RuleDto>,
}

#[derive(Debug, Deserialize)]
struct RuleDto {
    id: String,
    title: String,
    category: String,
    severity: String,
    #[serde(default)]
    cwe: Vec<String>,
    scope: String,
    engine: String,
    assertion: String,
}

/// A validated, in-memory ruleset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleManifest {
    /// Monotonic ruleset version, e.g. `2026.1.0-strict`.
    pub ruleset_version: String,
    /// ISO-8601 generation timestamp.
    pub generated_at: String,
    /// `total_rules` as declared by the manifest itself.
    pub declared_total_rules: usize,
    /// Declared category tokens.
    pub categories: Vec<String>,
    /// Ed25519 signature placeholder (verified in Phase 8).
    pub signature: String,
    /// The rules, in manifest order.
    pub rules: Vec<RuleSpec>,
}

impl RuleManifest {
    /// Load the manifest embedded in the binary at compile time.
    ///
    /// # Errors
    /// Returns [`DomainError`] if the embedded manifest cannot be parsed.
    pub fn embedded() -> Result<Self, DomainError> {
        Self::from_json(EMBEDDED_MANIFEST)
    }

    /// Load the manifest from disk.
    ///
    /// # Errors
    /// Returns [`DomainError`] if the file cannot be read or parsed.
    pub fn load_from_path(path: impl AsRef<Path>) -> Result<Self, DomainError> {
        let json = std::fs::read_to_string(path)?;
        Self::from_json(&json)
    }

    /// Parse a manifest from a JSON string.
    ///
    /// # Errors
    /// Returns [`DomainError`] if the JSON is malformed, a rule references an
    /// unknown category/severity/scope/engine, or the declared rule count does
    /// not match the number of rules present.
    pub fn from_json(json: &str) -> Result<Self, DomainError> {
        let dto: ManifestDto =
            serde_json::from_str(json).map_err(|error| DomainError::parse(error.to_string()))?;

        let rules = dto
            .rules
            .into_iter()
            .map(Self::map_rule)
            .collect::<Result<Vec<_>, _>>()?;

        let manifest = Self {
            ruleset_version: dto.ruleset_version,
            generated_at: dto.generated_at,
            declared_total_rules: dto.total_rules,
            categories: dto.categories,
            signature: dto.signature,
            rules,
        };
        manifest.validate()?;
        Ok(manifest)
    }

    fn map_rule(dto: RuleDto) -> Result<RuleSpec, DomainError> {
        Ok(RuleSpec {
            id: dto.id,
            title: dto.title,
            category: dto.category.parse::<FindingCategory>()?,
            severity: dto.severity.parse::<Severity>()?,
            cwe: dto.cwe,
            scope: dto.scope.parse::<RuleScope>()?,
            engine: dto.engine.parse::<EvaluationEngine>()?,
            assertion: dto.assertion,
        })
    }

    /// Check internal consistency of the manifest.
    ///
    /// # Errors
    /// Returns [`DomainError::Invariant`] when the declared count disagrees with
    /// the number of rules, when there are no rules, or when duplicate IDs exist.
    pub fn validate(&self) -> Result<(), DomainError> {
        if self.rules.len() != self.declared_total_rules {
            return Err(DomainError::invariant(format!(
                "manifest declares {} rules but contains {}",
                self.declared_total_rules,
                self.rules.len()
            )));
        }
        if self.rules.is_empty() {
            return Err(DomainError::invariant("manifest contains no rules"));
        }
        let mut seen = std::collections::HashSet::new();
        for rule in &self.rules {
            if !seen.insert(rule.id.as_str()) {
                return Err(DomainError::invariant(format!(
                    "duplicate rule id `{}`",
                    rule.id
                )));
            }
        }
        Ok(())
    }

    /// Number of rules in the manifest.
    pub fn len(&self) -> usize {
        self.rules.len()
    }

    /// `true` when the manifest carries no rules.
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// Look up a rule by its stable identifier.
    pub fn rule_by_id(&self, id: &str) -> Option<&RuleSpec> {
        self.rules.iter().find(|rule| rule.id == id)
    }
}
