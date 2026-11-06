use crate::category::FindingCategory;
use crate::rule::EvaluationEngine;
use crate::score::RiskScore;
use crate::severity::Severity;

/// An identified defect or architectural violation.
///
/// Pure domain semantics: it carries **no** presentation coordinates. The
/// polar radar projection is computed later by an interface adapter
/// (`RadarPresenter`) from these fields.
#[derive(Debug, Clone, PartialEq)]
pub struct Finding {
    /// Stable identifier for this finding.
    pub id: String,
    /// Path of the offending file.
    pub file_path: String,
    /// First offending line (1-based, inclusive).
    pub line_start: u32,
    /// Last offending line (1-based, inclusive).
    pub line_end: u32,
    /// Rule that produced the finding.
    pub rule_id: String,
    /// Review pillar.
    pub category: FindingCategory,
    /// Severity band.
    pub severity: Severity,
    /// Associated CWE identifier, when applicable.
    pub cwe: Option<String>,
    /// Short title.
    pub title: String,
    /// Longer explanation.
    pub description: String,
    /// Legacy evaluator score in `[0.0, 1.0]`; its meaning depends on the producer.
    pub confidence: f32,
    /// Raw model probability, when supplied by a model evaluator.
    pub model_probability: Option<f32>,
    /// Uncalibrated heuristic weight, when supplied by a heuristic evaluator.
    pub heuristic_weight: Option<f32>,
    /// Engine that actually produced the finding, as it crossed the
    /// evaluation boundary (`CR-SPEC-09 §5`).
    pub engine: EvaluationEngine,
    /// Normalized risk score in `[0.0, 1.0]`.
    pub risk_score: RiskScore,
}

impl Finding {
    /// Create a finding; `id` is derived deterministically from the rule and
    /// location so repeated scans are stable.
    pub fn new(
        rule_id: impl Into<String>,
        file_path: impl Into<String>,
        line_start: u32,
        line_end: u32,
    ) -> Self {
        let rule_id = rule_id.into();
        let file_path = file_path.into();
        let id = format!("{rule_id}:{file_path}:{line_start}");
        Self {
            id,
            file_path,
            line_start,
            line_end,
            rule_id,
            category: FindingCategory::default(),
            severity: Severity::default(),
            cwe: None,
            title: String::new(),
            description: String::new(),
            confidence: 1.0,
            model_probability: None,
            heuristic_weight: None,
            engine: EvaluationEngine::default(),
            risk_score: RiskScore::default(),
        }
    }

    /// Override the derived identifier.
    #[must_use]
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    /// Set the review pillar.
    #[must_use]
    pub const fn with_category(mut self, category: FindingCategory) -> Self {
        self.category = category;
        self
    }

    /// Set the severity band.
    #[must_use]
    pub const fn with_severity(mut self, severity: Severity) -> Self {
        self.severity = severity;
        self
    }

    /// Attach a CWE identifier.
    #[must_use]
    pub fn with_cwe(mut self, cwe: impl Into<String>) -> Self {
        self.cwe = Some(cwe.into());
        self
    }

    /// Set the short title.
    #[must_use]
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    /// Set the long description.
    #[must_use]
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    /// Set the evaluator confidence, clamped into `[0.0, 1.0]`.
    #[must_use]
    pub fn with_confidence(mut self, confidence: f32) -> Self {
        self.confidence = if confidence.is_nan() {
            0.0
        } else {
            confidence.clamp(0.0, 1.0)
        };
        self.model_probability = None;
        self.heuristic_weight = None;
        self
    }

    /// Record a model probability and retain the legacy score for existing consumers.
    #[must_use]
    pub fn with_model_probability(self, probability: f32) -> Self {
        let mut finding = self.with_confidence(probability);
        finding.model_probability = Some(finding.confidence);
        finding
    }

    /// Record an uncalibrated heuristic weight and retain the legacy score.
    #[must_use]
    pub fn with_heuristic_weight(self, weight: f32) -> Self {
        let mut finding = self.with_confidence(weight);
        finding.heuristic_weight = Some(finding.confidence);
        finding
    }

    /// Set the engine that produced the finding.
    #[must_use]
    pub const fn with_engine(mut self, engine: EvaluationEngine) -> Self {
        self.engine = engine;
        self
    }

    /// Set the normalized risk score.
    #[must_use]
    pub const fn with_risk_score(mut self, risk_score: RiskScore) -> Self {
        self.risk_score = risk_score;
        self
    }

    /// Number of lines the finding spans (inclusive).
    pub const fn line_span(&self) -> u32 {
        self.line_end.saturating_sub(self.line_start) + 1
    }

    /// `true` when the finding represents no real defect.
    pub fn is_clean(&self) -> bool {
        self.severity == Severity::Clean
    }
}
