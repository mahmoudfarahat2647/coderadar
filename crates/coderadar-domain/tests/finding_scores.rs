use coderadar_domain::Finding;

#[test]
fn model_probability_and_heuristic_weight_are_distinct() {
    let base = Finding::new("rule", "file.ts", 1, 1);
    assert_eq!(base.model_probability, None);
    assert_eq!(base.heuristic_weight, None);

    let model = base.clone().with_model_probability(0.73);
    assert_eq!(model.model_probability, Some(0.73));
    assert_eq!(model.heuristic_weight, None);
    assert_eq!(model.confidence, 0.73);

    let heuristic = base.with_heuristic_weight(0.6);
    assert_eq!(heuristic.model_probability, None);
    assert_eq!(heuristic.heuristic_weight, Some(0.6));
    assert_eq!(heuristic.confidence, 0.6);
    let replaced = heuristic.with_model_probability(0.8);
    assert_eq!(replaced.model_probability, Some(0.8));
    assert_eq!(replaced.heuristic_weight, None);
    let legacy = replaced.with_confidence(0.5);
    assert_eq!(legacy.model_probability, None);
    assert_eq!(legacy.heuristic_weight, None);
}
