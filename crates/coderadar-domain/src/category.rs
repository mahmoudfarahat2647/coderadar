use std::fmt;
use std::str::FromStr;

use crate::error::DomainError;

/// The five orthogonal review pillars (see `CR-SPEC-04`). Each pillar maps 1:1
/// to a UI tab and to a deterministic quadrant on the polar radar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum FindingCategory {
    /// Pillar 2 — Clean Architecture & SOLID (`ARCH-*`).
    CleanArchitecture,
    /// Pillar 1 — Security & Compliance (`SEC-*`).
    #[default]
    Security,
    /// Pillar 3 — Edge Cases & Robustness (`EDGE-*`).
    EdgeCases,
    /// Pillar 4 — Performance & Resource Leaks (`PERF-*`).
    Performance,
    /// Pillar 5 — Pull Request & Diff Hygiene (`PR-*`).
    PullRequests,
}

impl FindingCategory {
    /// Canonical lowercase token used by `rules/manifest.json`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CleanArchitecture => "architecture",
            Self::Security => "security",
            Self::EdgeCases => "edge_cases",
            Self::Performance => "performance",
            Self::PullRequests => "pull_requests",
        }
    }

    /// Rule-ID prefix carried by every rule in this pillar.
    pub const fn rule_prefix(self) -> &'static str {
        match self {
            Self::CleanArchitecture => "ARCH",
            Self::Security => "SEC",
            Self::EdgeCases => "EDGE",
            Self::Performance => "PERF",
            Self::PullRequests => "PR",
        }
    }

    /// Human-readable pillar name for the UI.
    pub const fn pillar_name(self) -> &'static str {
        match self {
            Self::CleanArchitecture => "Clean Architecture & SOLID",
            Self::Security => "Security & Compliance",
            Self::EdgeCases => "Edge Cases & Robustness",
            Self::Performance => "Performance & Resource Leaks",
            Self::PullRequests => "Pull Request & Diff Hygiene",
        }
    }

    /// Base angle (degrees) of this pillar's radar quadrant (see `CR-SPEC-02`).
    pub const fn radar_base_angle_deg(self) -> f32 {
        match self {
            Self::CleanArchitecture => 45.0,
            Self::Security => 135.0,
            // Held at 0.0 because it is handled by the dedicated PR-Diff mode.
            Self::PullRequests => 0.0,
            Self::EdgeCases => 315.0,
            Self::Performance => 225.0,
        }
    }

    /// All pillars in canonical order.
    pub const fn all() -> [Self; 5] {
        [
            Self::Security,
            Self::CleanArchitecture,
            Self::EdgeCases,
            Self::Performance,
            Self::PullRequests,
        ]
    }
}

impl fmt::Display for FindingCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for FindingCategory {
    type Err = DomainError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "architecture" | "clean_architecture" | "arch" | "cleanarch" => {
                Ok(Self::CleanArchitecture)
            }
            "security" | "sec" => Ok(Self::Security),
            "edge_cases" | "edgecases" | "edge" => Ok(Self::EdgeCases),
            "performance" | "perf" => Ok(Self::Performance),
            "pull_requests" | "pullrequests" | "pr" => Ok(Self::PullRequests),
            other => Err(DomainError::parse(format!("unknown category `{other}`"))),
        }
    }
}
