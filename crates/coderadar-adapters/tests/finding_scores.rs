use coderadar_adapters::{FindingDTO, RadarPresenter};
use coderadar_domain::Finding;

#[test]
fn finding_dto_preserves_score_meaning() {
    for finding in [
        Finding::new("model", "a.ts", 1, 1).with_model_probability(0.73),
        Finding::new("heuristic", "a.ts", 1, 1).with_heuristic_weight(0.6),
        Finding::new("legacy", "a.ts", 1, 1).with_confidence(0.9),
    ] {
        let dto = FindingDTO::from_finding(&finding, RadarPresenter::project_to_polar(&finding));
        assert_eq!(dto.model_probability, finding.model_probability);
        assert_eq!(dto.heuristic_weight, finding.heuristic_weight);
        assert_eq!(dto.confidence, Some(finding.confidence));
    }
}
