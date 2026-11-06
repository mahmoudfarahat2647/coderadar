//! Scan pipeline — wires the scanner to the evaluators with bounded memory.
//!
//! [`ScanPipeline::run`] walks the project through [`ParallelFileScanner`] and
//! evaluates chunks **in bounded batches** as they stream in, so peak memory
//! stays flat regardless of repository size (gate: 5,000 files, RSS < 45 MB).
//!
//! * AST chunks are buffered up to `ast_batch_size` and evaluated against
//!   ast-chunk-scope rules;
//! * whole-file chunks are buffered up to `module_batch_size` and evaluated
//!   against file/module-scope rules;
//! * findings are deduplicated by id and sorted deterministically, because the
//!   parallel walk delivers files in nondeterministic order.
//!
//! `git_changeset`-scoped rules are not requested in `FullScan` mode. A
//! `StagedDiff` run scans only the staged files and then evaluates the `PR-*`
//! rules through the [`AssessPullRequest`] use case, so the report carries both
//! the code findings and the Pull Request hygiene scorecard.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use coderadar_domain::{
    ChunkKind, CodeChunk, DomainError, EvaluationEngine, Finding, PullRequestHygiene, RuleScope,
    RuleSpec, ScanMode, ScanSession,
};
use coderadar_usecases::{AssessPullRequest, ChangesetGateway, ChunkSink, SemanticRuleEvaluator};

use crate::executor::block_on;
use crate::scanner::ParallelFileScanner;

/// Lowest accepted confidence of a finding produced by the mock heuristic
/// engine (`CR-SPEC-09 §5`).
///
/// Tier 0 static analysis is deterministic and never filtered; heuristic
/// findings below the floor are dropped and only counted, so untuned
/// codebases are not flooded with low-precision suggestions. The floor is
/// inclusive (a finding at exactly `0.5` survives) and derived from the Jev
/// Tier 1 self-gate so the two engines cannot drift apart.
pub const HEURISTIC_CONFIDENCE_FLOOR: f32 = crate::jev::DEFAULT_CONFIDENCE_THRESHOLD;

/// Outcome of a pipeline run: the session plus scan statistics.
#[derive(Debug, Clone, PartialEq)]
pub struct ScanOutcome {
    /// Completed scan session with every finding.
    pub session: ScanSession,
    /// Number of supported files that passed the bloat guard.
    pub files_scanned: usize,
    /// Number of AST-window chunks evaluated.
    pub ast_chunks: usize,
    /// Number of whole-file chunks evaluated.
    pub module_chunks: usize,
    /// Pull Request hygiene, present only for a `StagedDiff` run.
    pub pull_request: Option<PullRequestHygiene>,
}

/// Mutable state shared by every scanner worker thread.
///
/// The scanner requires the sink to be `Send + Sync`, so all buffers live
/// behind one mutex; chunks are moved in and batches moved out, the sink never
/// borrows from the walker.
#[derive(Default)]
struct SinkState {
    ast_buf: Vec<CodeChunk>,
    module_buf: Vec<CodeChunk>,
    ast_chunks: usize,
    module_chunks: usize,
    findings: Vec<Finding>,
    suppressed: usize,
    error: Option<DomainError>,
    sources: BTreeMap<String, String>,
    unverified_results: bool,
}

/// Callback invoked with batches of findings as they are evaluated.
pub type FindingBatchCallback = Arc<dyn Fn(&[Finding]) + Send + Sync>;

/// A [`ChunkSink`] that evaluates bounded batches as chunks stream in.
///
/// The sink owns everything it needs (`Arc`s for the evaluator and rule
/// slices) so it satisfies the scanner's `'static` sink requirement. The
/// first engine failure is latched into [`SinkState::error`] — the scanner
/// masks sink errors behind a generic message, so [`ScanPipeline::run`] reads
/// the real error from the sink *before* consulting the scanner result.
struct BatchingSink {
    evaluator: Arc<dyn SemanticRuleEvaluator>,
    ast_rules: Arc<[RuleSpec]>,
    module_rules: Arc<[RuleSpec]>,
    ast_batch_size: usize,
    module_batch_size: usize,
    state: Mutex<SinkState>,
    provisional: Option<FindingBatchCallback>,
}

impl BatchingSink {
    fn new(
        evaluator: Arc<dyn SemanticRuleEvaluator>,
        ast_rules: Vec<RuleSpec>,
        module_rules: Vec<RuleSpec>,
        ast_batch_size: usize,
        module_batch_size: usize,
        provisional: Option<FindingBatchCallback>,
    ) -> Self {
        Self {
            evaluator,
            ast_rules: ast_rules.into(),
            module_rules: module_rules.into(),
            ast_batch_size,
            module_batch_size,
            state: Mutex::new(SinkState::default()),
            provisional,
        }
    }

    /// Evaluate one batch, flatten the per-rule findings and apply the
    /// heuristic confidence floor (Tier 0 and Tier 1 results pass through).
    /// Returns the kept findings and how many were suppressed.
    fn evaluate_chunks(
        &self,
        batch: Vec<CodeChunk>,
        rules: &[RuleSpec],
    ) -> Result<(Vec<Finding>, usize, bool), DomainError> {
        let results = block_on(self.evaluator.evaluate_batch(batch, rules))?;
        let mut expected = BTreeMap::new();
        for rule in rules {
            *expected.entry(rule.id.as_str()).or_insert(0usize) += 1;
        }
        let mut unverified = false;
        for result in &results {
            match expected.get_mut(result.rule_id.as_str()) {
                Some(count) if *count > 0 => *count -= 1,
                _ => unverified = true,
            }
        }
        if expected.values().any(|count| *count != 0) {
            return Err(DomainError::invariant(
                "evaluator omitted a requested rule result",
            ));
        }
        let mut findings = Vec::new();
        let mut suppressed = 0;
        for result in results {
            if result.engine != EvaluationEngine::MockHeuristic {
                findings.extend(result.findings);
                continue;
            }
            for finding in result.findings {
                if finding.confidence >= HEURISTIC_CONFIDENCE_FLOOR {
                    findings.push(finding);
                } else {
                    suppressed += 1;
                }
            }
        }
        Ok((findings, suppressed, unverified))
    }

    /// Take and evaluate whatever remains buffered (end of the walk).
    fn flush(&self) -> Result<(), DomainError> {
        let (ast_batch, module_batch) = {
            let mut state = self.lock_state()?;
            (
                std::mem::take(&mut state.ast_buf),
                std::mem::take(&mut state.module_buf),
            )
        };

        let mut findings = Vec::new();
        let mut suppressed_total = 0;
        let mut unverified = false;
        if !ast_batch.is_empty() {
            let (batch_findings, suppressed, extra) =
                self.evaluate_chunks(ast_batch, &self.ast_rules)?;
            unverified |= extra;
            findings.extend(batch_findings);
            suppressed_total += suppressed;
        }
        if !module_batch.is_empty() {
            let (batch_findings, suppressed, extra) =
                self.evaluate_chunks(module_batch, &self.module_rules)?;
            unverified |= extra;
            findings.extend(batch_findings);
            suppressed_total += suppressed;
        }

        if let Some(callback) = &self.provisional {
            if !findings.is_empty() {
                callback(&findings);
            }
        }
        let mut state = self.lock_state()?;
        state.findings.extend(findings);
        state.suppressed += suppressed_total;
        state.unverified_results |= unverified;
        Ok(())
    }

    /// Lock the state, mapping poisoning to a domain error.
    fn lock_state(&self) -> Result<std::sync::MutexGuard<'_, SinkState>, DomainError> {
        self.state
            .lock()
            .map_err(|_| DomainError::External("chunk sink lock poisoned".to_string()))
    }
}

impl ChunkSink for BatchingSink {
    /// Buffer the chunk and evaluate its side's batch once it is full.
    ///
    /// After the first engine failure the sink latches the error and silently
    /// drops every further chunk, so one failure neither aborts the walk with
    /// a misleading scanner error nor floods the log.
    fn send_chunk(&self, chunk: CodeChunk) -> Result<(), DomainError> {
        let mut state = self.lock_state()?;
        if state.error.is_some() {
            return Ok(());
        }

        let (batch, rules): (Vec<CodeChunk>, &[RuleSpec]) = match chunk.kind {
            ChunkKind::AstWindow => {
                state.ast_chunks += 1;
                state.ast_buf.push(chunk);
                if state.ast_buf.len() < self.ast_batch_size {
                    return Ok(());
                }
                (std::mem::take(&mut state.ast_buf), &self.ast_rules)
            }
            ChunkKind::FileModule => {
                state.module_chunks += 1;
                state.sources.insert(
                    chunk.file_path.clone(),
                    crate::hashing::sha256_hex(&chunk.content),
                );
                state.module_buf.push(chunk);
                if state.module_buf.len() < self.module_batch_size {
                    return Ok(());
                }
                (std::mem::take(&mut state.module_buf), &self.module_rules)
            }
        };

        match self.evaluate_chunks(batch, rules) {
            Ok((findings, suppressed, unverified)) => {
                if let Some(callback) = &self.provisional {
                    if !findings.is_empty() {
                        callback(&findings);
                    }
                }
                state.findings.extend(findings);
                state.suppressed += suppressed;
                state.unverified_results |= unverified;
                Ok(())
            }
            Err(error) => {
                state.error = Some(error.clone());
                Err(error)
            }
        }
    }
}

/// Streaming scan orchestrator.
pub struct ScanPipeline {
    evaluator: Arc<dyn SemanticRuleEvaluator>,
    rules: Vec<RuleSpec>,
    ast_batch_size: usize,
    module_batch_size: usize,
    changesets: Option<Arc<dyn ChangesetGateway>>,
    staged_snapshot: Option<Arc<crate::git_changeset::StagedSnapshot>>,
    configuration_fingerprint: Option<Result<Option<String>, ()>>,
}

impl ScanPipeline {
    /// Default number of AST chunks per evaluation batch.
    pub const DEFAULT_AST_BATCH_SIZE: usize = 32;
    /// Default number of whole-file chunks per evaluation batch.
    pub const DEFAULT_MODULE_BATCH_SIZE: usize = 4;

    /// Create a pipeline from an evaluator and the active ruleset.
    pub fn new(evaluator: Arc<dyn SemanticRuleEvaluator>, rules: Vec<RuleSpec>) -> Self {
        Self {
            evaluator,
            rules,
            ast_batch_size: Self::DEFAULT_AST_BATCH_SIZE,
            module_batch_size: Self::DEFAULT_MODULE_BATCH_SIZE,
            changesets: None,
            staged_snapshot: None,
            configuration_fingerprint: None,
        }
    }

    pub(crate) fn with_configuration_fingerprint(
        mut self,
        fingerprint: Option<Result<Option<String>, ()>>,
    ) -> Self {
        self.configuration_fingerprint = fingerprint;
        self
    }

    /// Enable staged scans. The captured Git snapshot itself supplies the
    /// changeset so that hygiene and source analysis observe the same index.
    /// The supplied gateway is retained as a compatibility capability marker.
    #[must_use]
    pub fn with_changeset_gateway(mut self, gateway: Arc<dyn ChangesetGateway>) -> Self {
        self.changesets = Some(gateway);
        self
    }

    /// Use an already captured index for staged source and changeset analysis.
    #[must_use]
    pub fn with_staged_snapshot(
        mut self,
        snapshot: Arc<crate::git_changeset::StagedSnapshot>,
    ) -> Self {
        self.staged_snapshot = Some(snapshot);
        self
    }

    /// Override the AST batch size (tests use small batches).
    #[must_use]
    pub fn with_ast_batch_size(mut self, size: usize) -> Self {
        self.ast_batch_size = size.max(1);
        self
    }

    /// Run a full scan of `root`, publishing findings after source verification.
    ///
    /// # Errors
    /// Returns [`DomainError`] when the root is invalid or an evaluator fails.
    pub fn run_with_callback<F>(
        &self,
        root: &Path,
        on_findings: F,
    ) -> Result<ScanOutcome, DomainError>
    where
        F: Fn(&[Finding]) + Send + Sync + 'static,
    {
        self.run_internal(root, ScanMode::FullScan, Some(Arc::new(on_findings)), None)
    }

    /// Run a full scan of `root` without a findings callback.
    ///
    /// `git_changeset`-scoped rules are excluded from a `FullScan`: they need
    /// the Git diff of the changeset, which the pipeline only reads for a
    /// `StagedDiff` run.
    ///
    /// # Errors
    /// Returns [`DomainError`] when the root is invalid or an evaluator fails.
    pub fn run(&self, root: &Path) -> Result<ScanOutcome, DomainError> {
        self.run_internal(root, ScanMode::FullScan, None, None)
    }

    /// Run a scan of `root` in `mode`, publishing findings after source verification.
    ///
    /// # Errors
    /// Returns [`DomainError`] when the root is invalid, no changeset gateway
    /// is configured for a `StagedDiff` run, the Git changeset cannot be read,
    /// or an evaluator fails.
    pub fn run_mode_with_callback<F>(
        &self,
        root: &Path,
        mode: ScanMode,
        on_findings: F,
    ) -> Result<ScanOutcome, DomainError>
    where
        F: Fn(&[Finding]) + Send + Sync + 'static,
    {
        self.run_internal(root, mode, Some(Arc::new(on_findings)), None)
    }

    /// Run a scan with unverified interim findings. The returned outcome is authoritative.
    ///
    /// # Errors
    /// Returns an error if the scan cannot be completed.
    pub fn run_mode_with_provisional_callback<F>(
        &self,
        root: &Path,
        mode: ScanMode,
        callback: F,
    ) -> Result<ScanOutcome, DomainError>
    where
        F: Fn(&[Finding]) + Send + Sync + 'static,
    {
        self.run_internal(root, mode, None, Some(Arc::new(callback)))
    }

    /// Run a scan of `root` in `mode`.
    ///
    /// # Errors
    /// Returns [`DomainError`] when the root is invalid, no changeset gateway
    /// is configured for a `StagedDiff` run, the Git changeset cannot be read,
    /// or an evaluator fails.
    pub fn run_mode(&self, root: &Path, mode: ScanMode) -> Result<ScanOutcome, DomainError> {
        self.run_internal(root, mode, None, None)
    }

    fn run_internal(
        &self,
        root: &Path,
        mode: ScanMode,
        finding_sink: Option<FindingBatchCallback>,
        provisional: Option<FindingBatchCallback>,
    ) -> Result<ScanOutcome, DomainError> {
        let started_ms = now_ms();
        if mode == ScanMode::StagedDiff
            && self.changesets.is_none()
            && self.staged_snapshot.is_none()
        {
            return Err(DomainError::invariant(
                "a staged scan requires a configured Git changeset gateway",
            ));
        }

        let ast_rules: Vec<RuleSpec> = self
            .rules
            .iter()
            .filter(|rule| rule.scope == RuleScope::AstChunk)
            .cloned()
            .collect();
        let module_rules: Vec<RuleSpec> = self
            .rules
            .iter()
            .filter(|rule| rule.scope == RuleScope::FileModule)
            .cloned()
            .collect();

        let sink = BatchingSink::new(
            Arc::clone(&self.evaluator),
            ast_rules,
            module_rules,
            self.ast_batch_size,
            self.module_batch_size,
            provisional,
        );

        let root_str = root.to_string_lossy().to_string();
        let snapshot = if mode == ScanMode::StagedDiff {
            let snapshot = match &self.staged_snapshot {
                Some(snapshot) => Arc::clone(snapshot),
                None => Arc::new(crate::git_changeset::GitChangesetReader::capture_staged(
                    root,
                )?),
            };
            if !snapshot.belongs_to(root)? {
                return Err(DomainError::invariant(
                    "captured index belongs to a different repository",
                ));
            }
            Some(snapshot)
        } else {
            None
        };
        let scanner = ParallelFileScanner::new();
        let scan_result = if let Some(snapshot) = &snapshot {
            scanner.stream_staged_snapshot_with_coverage(snapshot, &sink)
        } else {
            scanner.stream_project_with_coverage(&root_str, mode, &sink)
        };

        // The scanner collapses every sink failure into a generic external
        // error, so the engine's real error must be read from the sink first.
        {
            let mut state = sink.lock_state()?;
            if let Some(error) = state.error.take() {
                return Err(error);
            }
        }
        let mut coverage = scan_result?;

        sink.flush()?;
        coverage.analyzed += coverage.incomplete;
        coverage.incomplete = 0;

        let (ast_chunks, module_chunks, suppressed, mut buffered, mut sources, mut unverified) = {
            let mut state = sink.lock_state()?;
            (
                state.ast_chunks,
                state.module_chunks,
                state.suppressed,
                std::mem::take(&mut state.findings),
                std::mem::take(&mut state.sources),
                state.unverified_results,
            )
        };

        let consumed_sources = sources.clone();
        if mode == ScanMode::FullScan {
            let canonical_root = std::fs::canonicalize(root).map_err(|error| {
                DomainError::External(format!("cannot verify scan root: {error}"))
            })?;
            sources.retain(|path, expected| {
                let path = canonical_root.join(path);
                let matches = std::fs::canonicalize(&path)
                    .ok()
                    .filter(|resolved| resolved.starts_with(&canonical_root))
                    .and_then(|resolved| std::fs::File::open(resolved).ok())
                    .and_then(|file| {
                        let mut bytes = Vec::new();
                        file.take(crate::scanner::MAX_FILE_BYTES + 1)
                            .read_to_end(&mut bytes)
                            .ok()?;
                        Some(
                            bytes.len() as u64 <= crate::scanner::MAX_FILE_BYTES
                                && crate::hashing::sha256_hex_bytes(&bytes) == *expected,
                        )
                    })
                    .unwrap_or(false);
                if !matches {
                    coverage.analyzed -= 1;
                    coverage.incomplete += 1;
                }
                matches
            });
            buffered.retain(|finding| {
                !consumed_sources.contains_key(&finding.file_path)
                    || sources.contains_key(&finding.file_path)
            });
        }
        unverified |= buffered
            .iter()
            .any(|finding| !consumed_sources.contains_key(&finding.file_path));
        if mode == ScanMode::FullScan {
            if let Some(expected) = &self.configuration_fingerprint {
                unverified |= expected.is_err()
                    || crate::layer_map::configuration_fingerprint(root) != *expected;
            }
        }
        // Invalid evaluator output or changed policy invalidates the scan's
        // evidence conservatively; unknown metadata is for legacy rows (§2–3).
        if unverified || crate::engine_rules::validate_ownership(&self.rules).is_err() {
            coverage.incomplete += coverage.analyzed;
            coverage.analyzed = 0;
            sources.clear();
            buffered.clear();
        }
        buffered.retain(|finding| sources.contains_key(&finding.file_path));

        let mut pull_request = None;
        let mut all_findings = buffered.clone();
        if let Some(snapshot) = snapshot {
            let gateway: Arc<dyn ChangesetGateway> = snapshot;
            let assessment =
                block_on(AssessPullRequest::new(gateway).assess(&root_str, &self.rules))?;
            all_findings.extend(assessment.findings);
            pull_request = Some(assessment.hygiene);
        }

        if let Some(callback) = finding_sink {
            let verified: Vec<_> = buffered
                .iter()
                .filter(|finding| sources.contains_key(&finding.file_path))
                .cloned()
                .collect();
            if !verified.is_empty() {
                callback(&verified);
            }
        }

        let mut deduped: BTreeMap<String, Finding> = BTreeMap::new();
        for finding in all_findings {
            deduped.entry(finding.id.clone()).or_insert(finding);
        }

        let mut findings: Vec<Finding> = deduped.into_values().collect();
        findings.sort_by(|left, right| {
            (
                &left.file_path,
                left.line_start,
                left.line_end,
                &left.rule_id,
            )
                .cmp(&(
                    &right.file_path,
                    right.line_start,
                    right.line_end,
                    &right.rule_id,
                ))
        });

        let finished_ms = now_ms();
        let root_dir = crate::workspace_guard::canonical_root(root);
        let mut session =
            ScanSession::new(format!("scan-{started_ms}"), root_dir, mode, started_ms)
                .with_files_scanned(coverage.analyzed)
                .with_suppressed_heuristic_findings(suppressed);
        session.coverage = Some(coverage);
        session.completion = if coverage.incomplete > 0
            || coverage.unreadable > 0
            || coverage.oversized > 0
            || coverage.invalid_encoding > 0
            || unverified
            || crate::engine_rules::validate_ownership(&self.rules).is_err()
        {
            Some("partial".to_string())
        } else {
            Some("complete".to_string())
        };
        session.analyzed_sources = Some(sources);
        session.consumed_sources = Some(consumed_sources);
        for finding in findings {
            session.push(finding);
        }
        session.finished_at_unix_ms = Some(finished_ms);

        Ok(ScanOutcome {
            session,
            files_scanned: coverage.analyzed,
            ast_chunks,
            module_chunks,
            pull_request,
        })
    }
}

/// Current wall-clock time as Unix milliseconds.
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as u64)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;
    use std::sync::Arc;

    use coderadar_domain::DomainError;
    use tempfile::TempDir;

    use super::ScanPipeline;
    use crate::layer_map::LayerMap;
    use crate::router::RuleEngineRouter;
    use crate::rules_manifest::RuleManifest;

    fn write(root: &Path, relative: &str, contents: &str) {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
    }

    fn build_pipeline(root: &Path) -> ScanPipeline {
        let manifest = RuleManifest::embedded().expect("embedded manifest parses");
        let router = RuleEngineRouter::new(Arc::new(LayerMap::load(root)));
        ScanPipeline::new(Arc::new(router), manifest.rules.clone())
    }

    #[test]
    fn the_confidence_floor_drops_low_precision_heuristic_findings() {
        let temp = TempDir::new().unwrap();
        write(
            temp.path(),
            "src/edges.ts",
            concat!(
                "export function renderAudit(timeline: string[]) {\n",
                "  document.write(timeline.join(\"\"));\n",
                "}\n",
                "export function pick(toolbox: Record<string, string>, key: string): string {\n",
                "  return toolbox[key];\n",
                "}\n",
            ),
        );

        let pipeline = build_pipeline(temp.path());
        let outcome = pipeline.run(temp.path()).expect("scan succeeds");

        assert_eq!(
            outcome.session.suppressed_heuristic_findings, 1,
            "suppressed: {:#?}",
            outcome.session.findings
        );
        assert!(
            outcome
                .session
                .findings
                .iter()
                .any(|finding| finding.rule_id == "SEC-XSS-004"),
            "the confident finding is kept: {:#?}",
            outcome.session.findings
        );
        assert!(
            outcome
                .session
                .findings
                .iter()
                .all(|finding| finding.rule_id != "EDGE-BND-003"),
            "the 0.4-confidence finding is dropped: {:#?}",
            outcome.session.findings
        );
    }

    #[test]
    fn run_flags_dependency_violations_deterministically() {
        let temp = TempDir::new().unwrap();
        write(
            temp.path(),
            ".coderadar.json",
            r#"{"layers":{"domain":["src/domain/**"]}}"#,
        );
        write(
            temp.path(),
            "src/domain/order.ts",
            "import express from \"express\";\n\nexport const x = 1;\n",
        );

        let pipeline = build_pipeline(temp.path());
        let first = pipeline.run(temp.path()).expect("first scan succeeds");
        let second = pipeline.run(temp.path()).expect("second scan succeeds");

        assert!(
            first
                .session
                .findings
                .iter()
                .any(|finding| finding.rule_id == "ARCH-DEP-001" && finding.line_start == 1),
            "expected an ARCH-DEP-001 finding: {:#?}",
            first.session.findings
        );
        assert_eq!(
            first.session.findings, second.session.findings,
            "repeated runs produce identical findings"
        );
        assert_eq!(first.files_scanned, 1);
        assert_eq!(first.module_chunks, 1);
        assert!(first.ast_chunks >= 1);
        assert_eq!(first.ast_chunks, second.ast_chunks);
        assert_eq!(first.module_chunks, second.module_chunks);
    }

    #[test]
    fn missing_root_is_reported_as_not_found() {
        let temp = TempDir::new().unwrap();
        let pipeline = build_pipeline(temp.path());
        let missing = temp.path().join("missing");

        let error = pipeline.run(&missing).expect_err("missing root fails");

        assert!(matches!(error, DomainError::NotFound(_)), "got {error:?}");
    }

    #[test]
    fn staged_configuration_and_sources_share_the_captured_index() {
        use coderadar_domain::ScanMode;
        use git2::Repository;
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        let repo = Repository::init(root).unwrap();
        write(
            root,
            ".coderadar.json",
            r#"{"layers":{"domain":["custom/**"]}}"#,
        );
        write(root, "custom/a.ts", "import express from 'express';\n");
        let mut index = repo.index().unwrap();
        index.add_path(Path::new(".coderadar.json")).unwrap();
        index.add_path(Path::new("custom/a.ts")).unwrap();
        index.write().unwrap();
        let snapshot =
            Arc::new(crate::git_changeset::GitChangesetReader::capture_staged(root).unwrap());
        write(root, ".coderadar.json", r#"{"layers":{"domain":[]}}"#);
        index.add_path(Path::new(".coderadar.json")).unwrap();
        index.write().unwrap();
        let map = LayerMap::load_staged(&snapshot).unwrap();
        let manifest = RuleManifest::embedded().unwrap();
        let pipeline = ScanPipeline::new(
            Arc::new(RuleEngineRouter::new(Arc::new(map))),
            manifest.rules.clone(),
        )
        .with_staged_snapshot(snapshot);
        let outcome = pipeline.run_mode(root, ScanMode::StagedDiff).unwrap();
        assert!(outcome
            .session
            .findings
            .iter()
            .any(|f| f.rule_id == "ARCH-DEP-001" && f.file_path == "custom/a.ts"));
    }
}
