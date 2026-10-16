use std::fmt;

use crate::error::DomainError;
use crate::severity::Severity;

/// Value object: a normalized risk score in `[0.0, 1.0]`.
///
/// `0.0` means benign, `1.0` means a critical hazard. The type can only be
/// constructed through validating or saturating constructors, so no code in any
/// layer can ever hold an out-of-range score.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct RiskScore(f32);

/// Value object: a normalized code-quality score in `[0.0, 1.0]`.
///
/// The inverse semantics of [`RiskScore`]: `1.0` is pristine, `0.0` is
/// thoroughly compromised.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct QualityScore(f32);

fn validate_unit(value: f32, label: &str) -> Result<(), DomainError> {
    if !value.is_finite() {
        return Err(DomainError::invariant(format!("{label} must be finite")));
    }
    if !(0.0..=1.0).contains(&value) {
        return Err(DomainError::invariant(format!(
            "{label} must be within [0.0, 1.0], got {value}"
        )));
    }
    Ok(())
}

impl RiskScore {
    /// Benign score.
    pub const ZERO: Self = Self(0.0);
    /// Maximum hazard score.
    pub const ONE: Self = Self(1.0);

    /// Construct a score, rejecting NaN, infinities and out-of-range values.
    pub fn new(value: f32) -> Result<Self, DomainError> {
        validate_unit(value, "risk score")?;
        Ok(Self(value))
    }

    /// Construct a score, clamping NaN to `0.0` and out-of-range values into
    /// `[0.0, 1.0]`. Useful when aggregating noisy heuristics.
    pub fn saturating(value: f32) -> Self {
        if value.is_nan() {
            return Self::ZERO;
        }
        Self(value.clamp(0.0, 1.0))
    }

    /// The underlying normalized value.
    pub const fn value(self) -> f32 {
        self.0
    }

    /// `true` when the score is exactly benign.
    pub const fn is_benign(self) -> bool {
        self.0 == 0.0
    }

    /// `true` when the score sits in the critical band.
    pub fn is_critical(self) -> bool {
        self.0 >= Severity::Critical.weight()
    }
}

impl fmt::Display for RiskScore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.3}", self.0)
    }
}

impl TryFrom<f32> for RiskScore {
    type Error = DomainError;

    fn try_from(value: f32) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<Severity> for RiskScore {
    fn from(severity: Severity) -> Self {
        Self(severity.weight())
    }
}

impl QualityScore {
    /// Worst possible quality.
    pub const ZERO: Self = Self(0.0);
    /// Pristine quality.
    pub const ONE: Self = Self(1.0);

    /// Construct a score, rejecting NaN, infinities and out-of-range values.
    pub fn new(value: f32) -> Result<Self, DomainError> {
        validate_unit(value, "quality score")?;
        Ok(Self(value))
    }

    /// Construct a score, clamping NaN to `0.0` and out-of-range values into
    /// `[0.0, 1.0]`.
    pub fn saturating(value: f32) -> Self {
        if value.is_nan() {
            return Self::ZERO;
        }
        Self(value.clamp(0.0, 1.0))
    }

    /// Derive a quality score from a risk score (`quality = 1 - risk`).
    pub fn from_risk(risk: RiskScore) -> Self {
        Self(1.0 - risk.value())
    }

    /// The underlying normalized value.
    pub const fn value(self) -> f32 {
        self.0
    }

    /// `true` when the code is pristine.
    pub const fn is_pristine(self) -> bool {
        self.0 == 1.0
    }
}

impl fmt::Display for QualityScore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.3}", self.0)
    }
}

impl TryFrom<f32> for QualityScore {
    type Error = DomainError;

    fn try_from(value: f32) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
