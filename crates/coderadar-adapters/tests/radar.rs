use coderadar_adapters::{compute_angle_jitter, FindingDTO, RadarPresenter, MAX_ANGLE_JITTER_DEG};
use coderadar_domain::{Finding, FindingCategory, RiskScore, Severity};

#[test]
fn radius_tracks_risk_score() {
    let finding = Finding::new("SEC-INJ-001", "src/db.ts", 12, 14)
        .with_risk_score(RiskScore::new(0.8).unwrap());
    let coordinate = RadarPresenter::project_to_polar(&finding);
    assert!((coordinate.radius - 0.8).abs() < f32::EPSILON);
}

#[test]
fn security_findings_land_in_their_quadrant() {
    let finding = Finding::new("SEC-INJ-001", "src/db.ts", 12, 14);
    let coordinate = RadarPresenter::project_to_polar(&finding);
    let base = FindingCategory::Security.radar_base_angle_deg();
    assert!((coordinate.angle_deg - base).abs() <= MAX_ANGLE_JITTER_DEG);
}

#[test]
fn jitter_is_deterministic_and_bounded() {
    let a = compute_angle_jitter("src/db.ts", 12, 35.0);
    let b = compute_angle_jitter("src/db.ts", 12, 35.0);
    assert!((a - b).abs() < f32::EPSILON);
    assert!(a.abs() <= 35.0);

    let c = compute_angle_jitter("src/other.ts", 12, 35.0);
    assert!((a - c).abs() > f32::EPSILON);
}

#[test]
fn angles_are_normalized_into_zero_to_360() {
    let finding =
        Finding::new("PR-BRK-002", "src/api.ts", 1, 2).with_category(FindingCategory::PullRequests);
    let coordinate = RadarPresenter::project_to_polar(&finding);
    assert!((0.0..360.0).contains(&coordinate.angle_deg));
}

#[test]
fn dto_carries_radar_coordinates_not_domain_coordinates() {
    let finding = Finding::new("SEC-INJ-001", "src/db.ts", 12, 14)
        .with_severity(Severity::Critical)
        .with_category(FindingCategory::Security)
        .with_cwe("CWE-89")
        .with_title("SQL injection")
        .with_description("Unsanitized user input concatenated to SQL statement")
        .with_risk_score(RiskScore::new(0.9).unwrap());
    let coordinate = RadarPresenter::project_to_polar(&finding);
    let dto = FindingDTO::from_finding(&finding, coordinate);

    assert_eq!(dto.id, finding.id);
    assert_eq!(dto.category, "security");
    assert_eq!(dto.severity, "CRITICAL");
    assert_eq!(dto.cwe.as_deref(), Some("CWE-89"));
    assert_eq!(
        dto.description,
        "Unsanitized user input concatenated to SQL statement"
    );
    assert_eq!(dto.radar_r, coordinate.radius);
    assert_eq!(dto.radar_theta, coordinate.angle_deg);
}
