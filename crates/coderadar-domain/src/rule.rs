use std::fmt;
use std::str::FromStr;

use crate::category::FindingCategory;
use crate::error::DomainError;
use crate::severity::Severity;

/// The architectural context a rule needs in order to be evaluated
/// (see `CR-SPEC-04 §8`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum RuleScope {
    /// Single function / class block, up to 300 LOC.
    #[default]
    AstChunk,
    /// Full-file imports plus the `.coderadar.json` layer map.
    FileModule,
    /// Git diff statistics, modified public APIs and accompanying tests.
    GitChangeset,
}

/// Which engine is expected to evaluate a rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum EvaluationEngine {
    /// Deterministic Rust static analysis (Tree-sitter + layer map).
    #[default]
    Tier0Static,
    /// Semantic evaluation via the Jev cloud / on-prem model.
    Tier1Semantic,
    /// Offline regex heuristic used while Tier 1 is unavailable.
    MockHeuristic,
}

/// The authoritative definition of a single review rule.
///
/// This is a pure domain entity: it contains no serialization attributes and is
/// hydrated from `rules/manifest.json` by an outer-layer adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleSpec {
    /// Stable rule identifier, e.g. `SEC-INJ-001`.
    pub id: String,
    /// Human-readable rule title.
    pub title: String,
    /// Pillar this rule belongs to.
    pub category: FindingCategory,
    /// Declared severity.
    pub severity: Severity,
    /// Associated Common Weakness Enumeration identifiers, e.g. `CWE-89`.
    pub cwe: Vec<String>,
    /// Context required to evaluate the rule.
    pub scope: RuleScope,
    /// Engine expected to evaluate the rule.
    pub engine: EvaluationEngine,
    /// Natural-language assertion handed to the semantic evaluator.
    pub assertion: String,
}

impl RuleSpec {
    /// `true` when the rule is rated [`Severity::Critical`].
    pub fn is_critical(&self) -> bool {
        self.severity == Severity::Critical
    }

    /// First CWE identifier, if any.
    pub fn primary_cwe(&self) -> Option<&str> {
        self.cwe.first().map(String::as_str)
    }
}

impl fmt::Display for RuleScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::AstChunk => "ast_chunk",
            Self::FileModule => "file_module",
            Self::GitChangeset => "git_changeset",
        })
    }
}

impl FromStr for RuleScope {
    type Err = DomainError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "ast_chunk" | "chunk" => Ok(Self::AstChunk),
            "file_module" | "file" | "module" => Ok(Self::FileModule),
            "git_changeset" | "changeset" | "diff" | "git" => Ok(Self::GitChangeset),
            other => Err(DomainError::parse(format!("unknown rule scope `{other}`"))),
        }
    }
}

impl EvaluationEngine {
    /// Canonical token used across boundaries.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Tier0Static => "tier0",
            Self::Tier1Semantic => "tier1",
            Self::MockHeuristic => "mock",
        }
    }
}

impl fmt::Display for EvaluationEngine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for EvaluationEngine {
    type Err = DomainError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "tier0" | "tier_0" | "static" => Ok(Self::Tier0Static),
            "tier1" | "tier_1" | "jev" | "semantic" => Ok(Self::Tier1Semantic),
            "mock" | "heuristic" => Ok(Self::MockHeuristic),
            other => Err(DomainError::parse(format!(
                "unknown evaluation engine `{other}`"
            ))),
        }
    }
}
