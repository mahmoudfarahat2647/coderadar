use coderadar_domain::Finding;

use crate::radar::PolarCoordinate;

/// Lightweight, boundary-safe view of a [`Finding`].
///
/// Inner entities are never serialized across a boundary (`CR-SPEC-02 §3`); the
/// webview receives this flat DTO instead.
#[derive(Debug, Clone, PartialEq)]
pub struct FindingDTO {
    /// Stable finding identifier.
    pub id: String,
    /// Offending file path.
    pub file: String,
    /// First offending line.
    pub line_start: u32,
    /// Last offending line.
    pub line_end: u32,
    /// Rule that produced the finding.
    pub rule_id: String,
    /// Severity token.
    pub severity: String,
    /// Associated CWE, when applicable.
    pub cwe: Option<String>,
    /// Short title.
    pub title: String,
    /// Polar radius for the radar.
    pub radar_r: f32,
    /// Polar angle (degrees) for the radar.
    pub radar_theta: f32,
}

impl FindingDTO {
    /// Translate a domain finding and its projected coordinate into a DTO.
    pub fn from_finding(finding: &Finding, coordinate: PolarCoordinate) -> Self {
        Self {
            id: finding.id.clone(),
            file: finding.file_path.clone(),
            line_start: finding.line_start,
            line_end: finding.line_end,
            rule_id: finding.rule_id.clone(),
            severity: finding.severity.as_str().to_string(),
            cwe: finding.cwe.clone(),
            title: finding.title.clone(),
            radar_r: coordinate.radius,
            radar_theta: coordinate.angle_deg,
        }
    }
}
