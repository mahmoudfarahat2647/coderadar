use coderadar_domain::{Finding, ScanMode, ScanSession};
use coderadar_infrastructure::{
    markdown_from_report, FindingSnapshot, LayerMapSource, RuleManifest, ScanOutcome, ScanReport,
    SqliteScanHistoryRepository,
};
use coderadar_usecases::ScanHistoryRepository;

mod common;

fn findings() -> Vec<Finding> {
    vec![
        Finding::new("MODEL", "model.ts", 1, 1).with_model_probability(0.75),
        Finding::new("HEURISTIC", "heuristic.ts", 1, 1).with_heuristic_weight(0.5),
        Finding::new("LEGACY", "legacy.ts", 1, 1).with_confidence(0.9),
    ]
}

#[test]
fn report_json_and_markdown_distinguish_score_provenance() {
    let mut session = ScanSession::new("scores", ".", ScanMode::FullScan, 0);
    for finding in findings() {
        session.push(finding);
    }
    let outcome = ScanOutcome {
        session,
        files_scanned: 3,
        ast_chunks: 0,
        module_chunks: 0,
        pull_request: None,
    };
    let report = ScanReport::from_scan(
        &outcome,
        &RuleManifest::embedded().unwrap(),
        LayerMapSource::Fallback,
        0,
    );
    let json = serde_json::to_value(&report).unwrap();
    let entries = json["findings"].as_array().unwrap();
    let model = entries.iter().find(|f| f["rule_id"] == "MODEL").unwrap();
    assert_eq!(model["model_probability"], 0.75);
    assert!(model.get("heuristic_weight").unwrap().is_null());
    let heuristic = entries
        .iter()
        .find(|f| f["rule_id"] == "HEURISTIC")
        .unwrap();
    assert_eq!(heuristic["heuristic_weight"], 0.5);
    assert!(heuristic.get("model_probability").unwrap().is_null());
    let legacy = entries.iter().find(|f| f["rule_id"] == "LEGACY").unwrap();
    assert!(legacy.get("model_probability").unwrap().is_null());
    assert!(legacy.get("heuristic_weight").unwrap().is_null());
    let markdown = markdown_from_report(&report);
    assert!(markdown.contains("**Model probability:** 0.75"));
    assert!(markdown.contains("**Heuristic weight:** 0.50"));
    assert_eq!(markdown.matches("**Model probability:**").count(), 1);
    assert_eq!(markdown.matches("**Heuristic weight:**").count(), 1);
}

#[test]
fn history_round_trips_score_provenance_and_legacy_unknowns() {
    let repository = SqliteScanHistoryRepository::open_in_memory().unwrap();
    let mut session = ScanSession::new("scores", ".", ScanMode::FullScan, 0);
    for finding in findings() {
        session.push(finding);
    }
    common::block_on(repository.save_scan_session(&session)).unwrap();
    let snapshots = repository.load_snapshots("scores").unwrap();
    let values = serde_json::to_value(&snapshots).unwrap();
    assert_eq!(values[0]["model_probability"], 0.75);
    assert!(values[0].get("heuristic_weight").unwrap().is_null());
    assert_eq!(values[1]["heuristic_weight"], 0.5);
    assert!(values[2].get("model_probability").unwrap().is_null());
    assert!(values[2].get("heuristic_weight").unwrap().is_null());

    let mut legacy = serde_json::to_value(&snapshots[2]).unwrap();
    let object = legacy.as_object_mut().unwrap();
    object.remove("model_probability");
    object.remove("heuristic_weight");
    let restored: FindingSnapshot = serde_json::from_value(legacy).unwrap();
    let restored = serde_json::to_value(restored).unwrap();
    assert!(restored.get("model_probability").unwrap().is_null());
    assert!(restored.get("heuristic_weight").unwrap().is_null());
    assert!((restored["confidence"].as_f64().unwrap() - 0.9).abs() < 0.001);
}
