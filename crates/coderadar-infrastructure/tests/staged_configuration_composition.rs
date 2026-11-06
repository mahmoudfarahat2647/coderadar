use std::{
    fs,
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

use coderadar_domain::ScanMode;
use coderadar_infrastructure::{
    scan_project_mode, scan_project_streaming_with_mode, MockRuleEvaluator,
};
use git2::Repository;

#[test]
fn staged_configuration_drives_batch_and_streaming_after_worktree_diverges() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let repo = Repository::init(root).unwrap();
    fs::create_dir(root.join("custom")).unwrap();
    fs::write(
        root.join(".coderadar.json"),
        r#"{"layers":{"domain":["custom/**"]}}"#,
    )
    .unwrap();
    fs::write(root.join("custom/a.ts"), "import express from 'express';\n").unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(Path::new(".coderadar.json")).unwrap();
    index.add_path(Path::new("custom/a.ts")).unwrap();
    index.write().unwrap();

    for deleted in [false, true] {
        if deleted {
            fs::remove_file(root.join(".coderadar.json")).unwrap();
            fs::remove_file(root.join("custom/a.ts")).unwrap();
        } else {
            fs::write(root.join(".coderadar.json"), r#"{"layers":{"domain":[]}}"#).unwrap();
            fs::write(root.join("custom/a.ts"), "export const safe = 1;\n").unwrap();
        }
        let batch = scan_project_mode(root, ScanMode::StagedDiff).unwrap();
        let emitted = Arc::new(Mutex::new(Vec::new()));
        let collected = Arc::clone(&emitted);
        let streaming = scan_project_streaming_with_mode(
            root,
            Arc::new(MockRuleEvaluator::new()),
            Duration::from_millis(1),
            ScanMode::StagedDiff,
            move |batch| collected.lock().unwrap().extend(batch.findings),
            || "mock".to_string(),
        )
        .unwrap();
        for report in [&batch, &streaming] {
            assert_eq!(report.layer_map_source, "config_file");
            assert_eq!(report.scan.files_scanned, 1);
            assert!(report
                .findings
                .iter()
                .any(|f| f.rule_id == "ARCH-DEP-001" && f.file == "custom/a.ts"));
            assert_eq!(report.pr_hygiene.as_ref().unwrap().additions, 2);
        }
        assert_eq!(
            serde_json::to_value(&batch.findings).unwrap(),
            serde_json::to_value(&streaming.findings).unwrap()
        );
        assert!(emitted
            .lock()
            .unwrap()
            .iter()
            .any(|f| f.rule_id == "ARCH-DEP-001" && f.file == "custom/a.ts"));
    }
}

#[test]
fn full_scan_configuration_changes_invalidate_batch_and_streamed_evidence() {
    use coderadar_domain::{CodeChunk, DomainError, RuleEvaluationResult, RuleSpec};
    use coderadar_infrastructure::scan_project_with;
    use coderadar_usecases::{BoxFuture, SemanticRuleEvaluator};
    struct ConfigEditor(std::path::PathBuf);
    impl SemanticRuleEvaluator for ConfigEditor {
        fn evaluate_batch<'a>(
            &'a self,
            chunks: Vec<CodeChunk>,
            rules: &'a [RuleSpec],
        ) -> BoxFuture<'a, Result<Vec<RuleEvaluationResult>, DomainError>> {
            Box::pin(async move {
                fs::write(&self.0, r#"{"layers":{"domain":[]}}"#).unwrap();
                MockRuleEvaluator::new().evaluate_batch(chunks, rules).await
            })
        }
    }
    for initially_present in [false, true] {
        for streaming in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            fs::create_dir(dir.path().join("domain")).unwrap();
            fs::write(
                dir.path().join("domain/a.ts"),
                "import express from 'express';\n",
            )
            .unwrap();
            let config = dir.path().join(".coderadar.json");
            if initially_present {
                fs::write(&config, r#"{"layers":{"domain":["domain/**"]}}"#).unwrap();
            }
            let evaluator = Arc::new(ConfigEditor(config));
            let emitted = Arc::new(Mutex::new(Vec::new()));
            let collected = emitted.clone();
            let report = if streaming {
                scan_project_streaming_with_mode(
                    dir.path(),
                    evaluator,
                    Duration::from_millis(1),
                    ScanMode::FullScan,
                    move |batch| collected.lock().unwrap().extend(batch.findings),
                    || "mock".to_string(),
                )
                .unwrap()
            } else {
                scan_project_with(dir.path(), evaluator, || "mock".to_string()).unwrap()
            };
            assert_eq!(report.scan.completion, Some("partial"));
            assert_eq!(report.scan.files_scanned, 0);
            assert_eq!(report.scan.compliance_percent, None);
            assert!(report.findings.is_empty());
            assert!(emitted.lock().unwrap().is_empty());
        }
    }
}
