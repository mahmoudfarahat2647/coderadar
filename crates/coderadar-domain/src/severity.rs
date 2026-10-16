use std::fmt;
use std::str::FromStr;

use crate::error::DomainError;

/// Strongly-typed severity of a [`crate::Finding`].
///
/// Declaration order is meaningful: `Ord` sorts from least to most severe so
/// that findings can be ranked without a lookup table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum Severity {
    /// No issue detected; the file is clean.
    #[default]
    Clean,
    /// Informational / hardening opportunity.
    Low,
    /// Should be fixed but not urgent.
    Medium,
    /// Serious defect that should block a release.
    High,
    /// Exploitable or catastrophic defect; must block a release.
    Critical,
}

impl Severity {
    /// Canonical uppercase token used by `rules/manifest.json`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Clean => "CLEAN",
            Self::Low => "LOW",
            Self::Medium => "MEDIUM",
            Self::High => "HIGH",
            Self::Critical => "CRITICAL",
        }
    }

    /// Normalized risk weight in `[0.0, 1.0]` used to seed [`crate::RiskScore`].
    pub const fn weight(self) -> f32 {
        match self {
            Self::Clean => 0.0,
            Self::Low => 0.25,
            Self::Medium => 0.5,
            Self::High => 0.75,
            Self::Critical => 1.0,
        }
    }

    /// `true` for every severity that represents an actual defect.
    pub const fn is_actionable(self) -> bool {
        !matches!(self, Self::Clean)
    }

    /// All severities, ordered least to most severe.
    pub const fn all() -> [Self; 5] {
        [
            Self::Clean,
            Self::Low,
            Self::Medium,
            Self::High,
            Self::Critical,
        ]
    }
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Severity {
    type Err = DomainError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_uppercase().as_str() {
            "CLEAN" | "INFO" => Ok(Self::Clean),
            "LOW" => Ok(Self::Low),
            "MEDIUM" | "MED" | "MODERATE" => Ok(Self::Medium),
            "HIGH" => Ok(Self::High),
            "CRITICAL" | "CRIT" => Ok(Self::Critical),
            other => Err(DomainError::parse(format!("unknown severity `{other}`"))),
        }
    }
}
