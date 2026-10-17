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
//! `git_changeset`-scoped rules are not requested in `FullScan` mode; a
//! `StagedDiff` scan is rejected until the Phase 8 Git engine exists.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use coderadar_domain::{
    ChunkKind, CodeChunk, DomainError, Finding, RuleScope, RuleSpec, ScanMode, ScanSession,
};
use coderadar_usecases::{ChunkSink, ProjectScannerGateway, SemanticRuleEvaluator};

use crate::executor::block_on;
use crate::scanner::ParallelFileScanner;

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
    error: Option<DomainError>,
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
    finding_sink: Option<FindingBatchCallback>,
    state: Mutex<SinkState>,
}

impl BatchingSink {
    fn new(
        evaluator: Arc<dyn SemanticRuleEvaluator>,
        ast_rules: Vec<RuleSpec>,
        module_rules: Vec<RuleSpec>,
        ast_batch_size: usize,
        module_batch_size: usize,
        finding_sink: Option<FindingBatchCallback>,
    ) -> Self {
        Self {
            evaluator,
            ast_rules: ast_rules.into(),
            module_rules: module_rules.into(),
            ast_batch_size,
            module_batch_size,
            finding_sink,
            state: Mutex::new(SinkState::default()),
        }
    }

    /// Evaluate one batch and flatten the per-rule findings.
    fn evaluate_chunks(
        &self,
        batch: Vec<CodeChunk>,
        rules: &[RuleSpec],
    ) -> Result<Vec<Finding>, DomainError> {
        let results = block_on(self.evaluator.evaluate_batch(batch, rules))?;
        Ok(results
            .into_iter()
            .flat_map(|result| result.findings)
            .collect())
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
        if !ast_batch.is_empty() {
            findings.extend(self.evaluate_chunks(ast_batch, &self.ast_rules)?);
        }
        if !module_batch.is_empty() {
            findings.extend(self.evaluate_chunks(module_batch, &self.module_rules)?);
        }

        if let Some(sink) = &self.finding_sink {
            if !findings.is_empty() {
                sink(&findings);
            }
        }

        let mut state = self.lock_state()?;
        state.findings.extend(findings);
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
                state.module_buf.push(chunk);
                if state.module_buf.len() < self.module_batch_size {
                    return Ok(());
                }
                (std::mem::take(&mut state.module_buf), &self.module_rules)
            }
        };

        match self.evaluate_chunks(batch, rules) {
            Ok(findings) => {
                if let Some(sink) = &self.finding_sink {
                    if !findings.is_empty() {
                        sink(&findings);
                    }
                }
                state.findings.extend(findings);
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
        }
    }

    /// Override the AST batch size (tests use small batches).
    #[must_use]
    pub fn with_ast_batch_size(mut self, size: usize) -> Self {
        self.ast_batch_size = size.max(1);
        self
    }

    /// Run a full scan of `root`, notifying `on_findings` as each batch evaluates.
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
        self.run_internal(root, Some(Arc::new(on_findings)))
    }

    /// Run a full scan of `root`.
    ///
    /// `git_changeset`-scoped rules are excluded from a `FullScan`: they need
    /// the Git diff of the changeset, which the Phase 8 engine provides; the
    /// pipeline simply never requests them here.
    ///
    /// # Errors
    /// Returns [`DomainError`] when the root is invalid or an evaluator fails.
    pub fn run(&self, root: &Path) -> Result<ScanOutcome, DomainError> {
        self.run_internal(root, None)
    }

    fn run_internal(
        &self,
        root: &Path,
        finding_sink: Option<FindingBatchCallback>,
    ) -> Result<ScanOutcome, DomainError> {
        let started_ms = now_ms();

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
            finding_sink,
        );

        let root_str = root.to_string_lossy().to_string();
        let scan_result =
            ParallelFileScanner::new().stream_project_chunks(&root_str, ScanMode::FullScan, &sink);

        // The scanner collapses every sink failure into a generic external
        // error, so the engine's real error must be read from the sink first.
        {
            let mut state = sink.lock_state()?;
            if let Some(error) = state.error.take() {
                return Err(error);
            }
        }
        scan_result?;

        sink.flush()?;

        let mut deduped: BTreeMap<String, Finding> = BTreeMap::new();
        let (ast_chunks, module_chunks, buffered) = {
            let mut state = sink.lock_state()?;
            (
                state.ast_chunks,
                state.module_chunks,
                std::mem::take(&mut state.findings),
            )
        };
        for finding in buffered {
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
        let root_dir = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
        let root_dir = root_dir.to_string_lossy().replace('\\', "/");
        let mut session = ScanSession::new(
            format!("scan-{started_ms}"),
            root_dir,
            ScanMode::FullScan,
            started_ms,
        );
        for finding in findings {
            session.push(finding);
        }
        session.finished_at_unix_ms = Some(finished_ms);

        Ok(ScanOutcome {
            session,
            files_scanned: module_chunks,
            ast_chunks,
            module_chunks,
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
}
