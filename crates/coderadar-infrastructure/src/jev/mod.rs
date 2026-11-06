//! Jev Tier 1 semantic rule evaluator (`CR-SPEC-03`, `CR-SPEC-12 §3–§5`).
//!
//! [`JevRuleEvaluatorAdapter`] implements the unchanged Layer 2
//! [`SemanticRuleEvaluator`] port, so swapping it in touches no use case. It
//! speaks the typed-decision protocol measured in Phase 0
//! (`CR-SPEC-08 §3.4`): one `state` per chunk plus a map of typed `questions`,
//! answered with calibrated probabilities and **zero generated text**.
//!
//! # Privacy first
//!
//! Every outbound `state` is built from a chunk that has already passed through
//! [`crate::redaction`], so credentials are replaced with `[REDACTED_SECRET]`
//! before the payload leaves the machine.
//!
//! # Throughput
//!
//! Chunks are processed in batches of [`JevConfig::batch_size`] (10 by default)
//! by a bounded worker pool of up to [`JevConfig::max_parallel`] threads sharing
//! one HTTP/2 connection pool — one decision request per chunk, matching the
//! `CR-SPEC-08 §3.4` recommendation. Each request carries a [`JevConfig`]
//! timeout (2 s); transient failures (network, timeout, `429`, `5xx`) are
//! retried up to [`JevConfig::max_retries`] times, while permanent failures
//! (invalid key `4xx`, unparsable body) fail immediately.

use std::collections::BTreeMap;
use std::io::Read;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use coderadar_domain::{
    CodeChunk, DomainError, EvaluationEngine, Finding, RuleEvaluationResult, RuleSpec,
};
use coderadar_usecases::{BoxFuture, SemanticRuleEvaluator};
use serde::{Deserialize, Deserializer, Serialize};

use crate::engine::EngineStatus;
use crate::engine_rules::is_semantic_rule;
use crate::heuristics::build_finding;
use crate::redaction::redact_chunk;

/// Working OpenRouter decisions endpoint (`CR-SPEC-08 §2.1`).
pub const DEFAULT_ENDPOINT: &str = "https://openrouter.ai/api/alpha/decisions";
/// Model alias resolved server-side to the pinned Jev build.
pub const DEFAULT_MODEL: &str = "~typesafe/jev-latest";

/// Default request timeout: the issue's hard ceiling is 2 s.
pub const DEFAULT_TIMEOUT_MS: u64 = 2_000;
/// Default number of chunks per batch.
pub const DEFAULT_BATCH_SIZE: usize = 10;
/// Default size of the parallel worker pool.
pub const DEFAULT_MAX_PARALLEL: usize = 16;
/// Probability above which a `noul` answer is reported as a finding.
pub const DEFAULT_CONFIDENCE_THRESHOLD: f32 = 0.5;

/// Knobs for the Jev adapter.
#[derive(Debug, Clone)]
pub struct JevConfig {
    /// Decisions endpoint URL.
    pub endpoint: String,
    /// Model alias sent in the request body.
    pub model: String,
    /// Bearer token (OpenRouter or TypeSafe key).
    pub api_key: String,
    /// Per-request timeout; the issue caps this at 2 s.
    pub request_timeout: Duration,
    /// Retries after the first attempt.
    pub max_retries: u32,
    /// Chunks per batch.
    pub batch_size: usize,
    /// Maximum worker threads per batch.
    pub max_parallel: usize,
    /// Minimum `noul` probability to emit a finding.
    pub confidence_threshold: f32,
}

impl Default for JevConfig {
    fn default() -> Self {
        Self {
            endpoint: DEFAULT_ENDPOINT.to_string(),
            model: DEFAULT_MODEL.to_string(),
            api_key: String::new(),
            request_timeout: Duration::from_millis(DEFAULT_TIMEOUT_MS),
            max_retries: 2,
            batch_size: DEFAULT_BATCH_SIZE,
            max_parallel: DEFAULT_MAX_PARALLEL,
            confidence_threshold: DEFAULT_CONFIDENCE_THRESHOLD,
        }
    }
}

/// A typed question submitted to Jev.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum JevQuestion {
    /// Boolean-probability question: `noul` is the probability of "yes".
    Noul {
        /// Natural-language assertion the model must evaluate against the state.
        instructions: String,
    },
}

/// One decision request: a single `state` plus its typed questions.
#[derive(Debug, Clone, Serialize)]
pub struct JevRequest {
    /// Model alias.
    pub model: String,
    /// Redacted chunk rendered as textual state.
    pub state: String,
    /// Question name → typed question (question name is the rule id).
    pub questions: BTreeMap<String, JevQuestion>,
}

/// A typed answer returned by Jev.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum JevAnswer {
    /// `noul` probability answer.
    Noul {
        /// Probability of "yes" in `[0.0, 1.0]`.
        #[serde(deserialize_with = "deserialize_probability")]
        noul: f32,
    },
    /// Answer type beyond the Phase 5 protocol; parsed and ignored.
    #[serde(other)]
    Other,
}

fn deserialize_probability<'de, D: Deserializer<'de>>(deserializer: D) -> Result<f32, D::Error> {
    let value = f64::deserialize(deserializer)?;
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        return Err(serde::de::Error::custom("invalid Jev probability"));
    }
    Ok(value as f32)
}

impl JevAnswer {
    /// The calibrated confidence carried by this answer, if any.
    pub fn confidence(&self) -> Option<f32> {
        match self {
            Self::Noul { noul } => Some(*noul),
            Self::Other => None,
        }
    }
}

/// The typed-answer envelope returned by the decisions endpoint.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct JevResponse {
    /// Answer map keyed by question name (rule id).
    #[serde(deserialize_with = "deserialize_answers")]
    pub answers: BTreeMap<String, JevAnswer>,
}

impl JevResponse {
    pub(crate) fn matches_questions(&self, questions: &BTreeMap<String, JevQuestion>) -> bool {
        self.answers.len() == questions.len()
            && self.answers.iter().all(|(id, answer)| {
                questions.contains_key(id)
                    && matches!(answer, JevAnswer::Noul { noul } if noul.is_finite() && (0.0..=1.0).contains(noul))
            })
    }
}

fn deserialize_answers<'de, D>(deserializer: D) -> Result<BTreeMap<String, JevAnswer>, D::Error>
where
    D: Deserializer<'de>,
{
    struct AnswersVisitor;
    impl<'de> serde::de::Visitor<'de> for AnswersVisitor {
        type Value = BTreeMap<String, JevAnswer>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a map of uniquely keyed answers")
        }
        fn visit_map<M: serde::de::MapAccess<'de>>(
            self,
            mut map: M,
        ) -> Result<Self::Value, M::Error> {
            let mut answers = BTreeMap::new();
            while let Some((key, answer)) = map.next_entry::<String, JevAnswer>()? {
                if answers.insert(key, answer).is_some() {
                    return Err(serde::de::Error::custom("duplicate Jev answer key"));
                }
            }
            Ok(answers)
        }
    }
    deserializer.deserialize_map(AnswersVisitor)
}

/// The transport that performs one decision round trip.
///
/// Abstracting the transport lets tests exercise batching, retries, parsing and
/// fallback without any network access.
pub trait JevTransport: Send + Sync {
    /// Perform a single decision request.
    ///
    /// # Errors
    /// Returns [`JevTransportError::Transient`] for failures worth retrying
    /// (network faults, timeouts, `429`, `5xx`) and
    /// [`JevTransportError::Permanent`] for failures retrying cannot fix
    /// (invalid key, rejected payload, unparsable body).
    fn decide(&self, request: &JevRequest) -> Result<JevResponse, JevTransportError>;
}

/// Transport failures classified for the retry policy.
#[derive(Debug, Clone, PartialEq)]
pub enum JevTransportError {
    /// Network failure, timeout, `429` or `5xx`: retrying may succeed.
    Transient(String),
    /// Bad key (`4xx`), rejected payload or unparsable body: retrying cannot succeed.
    Permanent(String),
}

impl JevTransportError {
    /// The human-readable message carried by this error.
    pub fn message(&self) -> &str {
        match self {
            Self::Transient(message) | Self::Permanent(message) => message,
        }
    }
}

impl From<JevTransportError> for DomainError {
    fn from(error: JevTransportError) -> Self {
        DomainError::External(
            match error {
                JevTransportError::Transient(_) => "Jev request failed temporarily",
                JevTransportError::Permanent(_) => "Jev request failed permanently",
            }
            .to_string(),
        )
    }
}

/// `reqwest` blocking transport with an HTTP/2 connection pool.
pub struct HttpJevTransport {
    client: reqwest::blocking::Client,
    endpoint: String,
    api_key: String,
    timeout: Duration,
}

impl HttpJevTransport {
    /// Build the client (connection pool + 2 s budget) from `config`.
    ///
    /// # Errors
    /// Returns [`DomainError::External`] when the HTTP client cannot be built.
    pub fn new(config: &JevConfig) -> Result<Self, DomainError> {
        let client = reqwest::blocking::Client::builder()
            .timeout(config.request_timeout)
            .connect_timeout(config.request_timeout)
            .pool_max_idle_per_host(config.max_parallel)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|error| {
                DomainError::External(format!("failed to build Jev HTTP client: {error}"))
            })?;
        Ok(Self {
            client,
            endpoint: config.endpoint.clone(),
            api_key: config.api_key.clone(),
            timeout: config.request_timeout,
        })
    }

    /// The configured per-request timeout.
    pub fn request_timeout(&self) -> Duration {
        self.timeout
    }
}

impl JevTransport for HttpJevTransport {
    fn decide(&self, request: &JevRequest) -> Result<JevResponse, JevTransportError> {
        let response = self
            .client
            .post(&self.endpoint)
            .bearer_auth(&self.api_key)
            .json(request)
            .send()
            .map_err(|_| JevTransportError::Transient("Jev request failed".to_string()))?;

        let status = response.status();
        const MAX_RESPONSE_BYTES: u64 = 256 * 1024;
        let mut body = Vec::new();
        response
            .take(MAX_RESPONSE_BYTES + 1)
            .read_to_end(&mut body)
            .map_err(|_| JevTransportError::Transient("Jev body read failed".into()))?;
        if body.len() as u64 > MAX_RESPONSE_BYTES {
            return Err(JevTransportError::Permanent(
                "Jev response exceeds size limit".into(),
            ));
        }

        if !status.is_success() {
            let message = format!("Jev HTTP {}", status.as_u16());
            return Err(if status.as_u16() == 429 || status.is_server_error() {
                JevTransportError::Transient(message)
            } else {
                JevTransportError::Permanent(message)
            });
        }

        serde_json::from_slice::<JevResponse>(&body)
            .map_err(|_| JevTransportError::Permanent("Jev response parse failed".into()))
    }
}

/// Per-chunk evaluation outcome.
struct ChunkEvaluation {
    index: usize,
    answers: Vec<(String, f32)>,
}

/// Jev-backed semantic evaluator over the Layer 2 port.
pub struct JevRuleEvaluatorAdapter {
    config: JevConfig,
    transport: Arc<dyn JevTransport>,
    status: Arc<EngineStatus>,
}

impl JevRuleEvaluatorAdapter {
    /// Build the adapter with the real `reqwest` transport.
    ///
    /// # Errors
    /// Returns [`DomainError::External`] when the HTTP client cannot be built.
    pub fn new(config: JevConfig, status: Arc<EngineStatus>) -> Result<Self, DomainError> {
        let transport = Arc::new(HttpJevTransport::new(&config)?);
        Ok(Self {
            config,
            transport,
            status,
        })
    }

    /// Build the adapter over an injected transport (tests / on-prem).
    pub fn with_transport(
        config: JevConfig,
        transport: Arc<dyn JevTransport>,
        status: Arc<EngineStatus>,
    ) -> Self {
        Self {
            config,
            transport,
            status,
        }
    }

    /// The configured per-request timeout.
    pub fn request_timeout(&self) -> Duration {
        self.config.request_timeout
    }

    /// Render a redacted chunk as the Jev `state` string.
    fn build_state(chunk: &CodeChunk) -> String {
        format!(
            "FILE: {}\nLANGUAGE: {}\nKIND: {:?}\nLINES: {}-{}\n---\n{}\n",
            chunk.file_path,
            chunk.language,
            chunk.kind,
            chunk.start_line,
            chunk.end_line,
            chunk.content
        )
    }

    /// Evaluate one chunk, retrying transient failures.
    fn evaluate_one(
        &self,
        index: usize,
        chunk: &CodeChunk,
        questions: &BTreeMap<String, JevQuestion>,
    ) -> Result<ChunkEvaluation, DomainError> {
        let redacted = redact_chunk(chunk);
        let request = JevRequest {
            model: self.config.model.clone(),
            state: Self::build_state(&redacted),
            questions: questions.clone(),
        };

        let mut attempt = 0;
        let response = loop {
            let started = Instant::now();
            match self.transport.decide(&request) {
                Ok(response) => {
                    self.status
                        .record_tier1_request(started.elapsed().as_millis() as u64);
                    break response;
                }
                Err(error) => {
                    if matches!(error, JevTransportError::Transient(_))
                        && attempt < self.config.max_retries
                    {
                        attempt += 1;
                        std::thread::sleep(Duration::from_millis(50 * u64::from(attempt)));
                    } else {
                        return Err(DomainError::from(error));
                    }
                }
            }
        };

        if !response.matches_questions(questions) {
            return Err(DomainError::External(
                "invalid or incomplete Jev answers".to_string(),
            ));
        }

        let answers = response
            .answers
            .into_iter()
            .filter_map(|(rule_id, answer)| {
                let confidence = answer.confidence()?;
                (confidence >= self.config.confidence_threshold).then_some((rule_id, confidence))
            })
            .collect();

        Ok(ChunkEvaluation { index, answers })
    }

    /// Evaluate every chunk in batches, up to `max_parallel` workers at a time.
    fn evaluate_all(
        &self,
        chunks: &[CodeChunk],
        questions: &BTreeMap<String, JevQuestion>,
    ) -> Result<Vec<ChunkEvaluation>, DomainError> {
        let mut evaluations = Vec::with_capacity(chunks.len());
        let batch_size = self.config.batch_size.max(1);

        let mut start = 0;
        while start < chunks.len() {
            let end = (start + batch_size).min(chunks.len());
            let batch = &chunks[start..end];

            let counter = AtomicUsize::new(0);
            let results = thread::scope(|scope| -> Result<Vec<ChunkEvaluation>, DomainError> {
                let next = &counter;
                let workers = self.config.max_parallel.min(batch.len()).max(1);
                let mut handles = Vec::with_capacity(workers);
                for _ in 0..workers {
                    handles.push(scope.spawn(move || {
                        let mut local = Vec::new();
                        loop {
                            let index = next.fetch_add(1, Ordering::Relaxed);
                            if index >= batch.len() {
                                break;
                            }
                            local.push(self.evaluate_one(start + index, &batch[index], questions));
                        }
                        local
                    }));
                }
                let mut collected = Vec::with_capacity(batch.len());
                for handle in handles {
                    // A panicked worker loses only its own batch; the whole
                    // batch is re-evaluated by the Mock fallback instead of
                    // aborting the process.
                    let local = handle.join().map_err(|_| {
                        DomainError::External("a Jev worker thread panicked".to_string())
                    })?;
                    collected.extend(local);
                }
                collected
                    .into_iter()
                    .collect::<Result<Vec<ChunkEvaluation>, DomainError>>()
            });

            for result in results? {
                evaluations.push(result);
            }
            start = end;
        }

        Ok(evaluations)
    }
}

impl SemanticRuleEvaluator for JevRuleEvaluatorAdapter {
    fn evaluate_batch<'a>(
        &'a self,
        chunks: Vec<CodeChunk>,
        rules: &'a [RuleSpec],
    ) -> BoxFuture<'a, Result<Vec<RuleEvaluationResult>, DomainError>> {
        Box::pin(async move {
            let mut findings_per_rule: Vec<Vec<Finding>> =
                (0..rules.len()).map(|_| Vec::new()).collect();

            let questions: BTreeMap<String, JevQuestion> = rules
                .iter()
                .filter(|rule| is_semantic_rule(rule))
                .map(|rule| {
                    (
                        rule.id.clone(),
                        JevQuestion::Noul {
                            instructions: rule.assertion.clone(),
                        },
                    )
                })
                .collect();

            if questions.len() != rules.iter().filter(|rule| is_semantic_rule(rule)).count() {
                return Err(DomainError::invariant("duplicate Jev rule id"));
            }

            if !questions.is_empty() && !chunks.is_empty() {
                for evaluation in self.evaluate_all(&chunks, &questions)? {
                    let chunk = &chunks[evaluation.index];
                    for (rule_id, confidence) in evaluation.answers {
                        let Some(position) = rules.iter().position(|rule| rule.id == rule_id)
                        else {
                            continue;
                        };
                        let rule = &rules[position];
                        findings_per_rule[position].push(
                            build_finding(
                                rule,
                                &chunk.file_path,
                                chunk.start_line,
                                chunk.end_line,
                                describe(rule, confidence),
                                confidence,
                            )
                            .with_model_probability(confidence),
                        );
                    }
                }
            }

            Ok(rules
                .iter()
                .enumerate()
                .map(|(index, rule)| {
                    let findings = std::mem::take(&mut findings_per_rule[index])
                        .into_iter()
                        .map(|finding| finding.with_engine(EvaluationEngine::Tier1Semantic))
                        .collect();
                    RuleEvaluationResult::new(
                        rule.id.clone(),
                        EvaluationEngine::Tier1Semantic,
                        findings,
                    )
                })
                .collect())
        })
    }
}

/// Build the generic, source-free description attached to a Jev finding.
fn describe(rule: &RuleSpec, confidence: f32) -> String {
    format!(
        "Jev Tier 1 semantic match (model probability {:.0}%): {}",
        confidence * 100.0,
        rule.assertion
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use coderadar_domain::{
        CodeChunk, DomainError, EvaluationEngine, RuleEvaluationResult, RuleSpec,
    };
    use coderadar_usecases::SemanticRuleEvaluator;

    use super::{
        HttpJevTransport, JevAnswer, JevConfig, JevRequest, JevResponse, JevRuleEvaluatorAdapter,
        JevTransport, JevTransportError,
    };
    use crate::engine::{EngineMode, EngineStatus};
    use crate::redaction::REDACTED_SECRET;
    use crate::rules_manifest::RuleManifest;

    #[test]
    fn wire_probabilities_are_checked_before_float_narrowing() {
        for value in ["1.0000000000000002", "-1e-50"] {
            let json = format!(r#"{{"answers":{{"probe":{{"type":"noul","noul":{value}}}}}}}"#);
            assert!(
                serde_json::from_str::<JevResponse>(&json).is_err(),
                "{value}"
            );
        }
    }

    #[test]
    fn injected_transport_errors_do_not_expose_sensitive_content() {
        let adapter = adapter_with(
            JevConfig {
                max_retries: 0,
                ..JevConfig::default()
            },
            Arc::new(ScriptedTransport {
                answers: Mutex::new(BTreeMap::new()),
                calls: AtomicUsize::new(0),
                error: Some(JevTransportError::Permanent(
                    "secret source and credential".into(),
                )),
            }),
        );
        let error = crate::executor::block_on(adapter.evaluate_batch(
            vec![CodeChunk::new("a.ts", "typescript", 1, 1, "x")],
            &[rule("SEC-JWT-003")],
        ))
        .unwrap_err();
        assert!(!format!("{error:?}").contains("secret"));
    }

    /// Transport with scripted answers and a recorded request log.
    struct ScriptedTransport {
        answers: Mutex<BTreeMap<String, JevAnswer>>,
        calls: AtomicUsize,
        error: Option<JevTransportError>,
    }

    impl ScriptedTransport {
        fn firing(rule_id: &str) -> Self {
            let mut answers = BTreeMap::new();
            answers.insert(rule_id.to_string(), JevAnswer::Noul { noul: 0.92 });
            Self {
                answers: Mutex::new(answers),
                calls: AtomicUsize::new(0),
                error: None,
            }
        }

        fn clean() -> Self {
            Self {
                answers: Mutex::new(BTreeMap::new()),
                calls: AtomicUsize::new(0),
                error: None,
            }
        }

        fn failing() -> Self {
            Self {
                answers: Mutex::new(BTreeMap::new()),
                calls: AtomicUsize::new(0),
                error: Some(JevTransportError::Transient("offline".to_string())),
            }
        }

        fn permanent() -> Self {
            Self {
                answers: Mutex::new(BTreeMap::new()),
                calls: AtomicUsize::new(0),
                error: Some(JevTransportError::Permanent(
                    "Jev HTTP 401: invalid key".to_string(),
                )),
            }
        }
    }

    impl JevTransport for ScriptedTransport {
        fn decide(&self, request: &JevRequest) -> Result<JevResponse, JevTransportError> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            if let Some(error) = self.error.clone() {
                return Err(error);
            }
            let mut answers = self.answers.lock().expect("answers lock").clone();
            for id in request.questions.keys() {
                answers
                    .entry(id.clone())
                    .or_insert(JevAnswer::Noul { noul: 0.0 });
            }
            Ok(JevResponse { answers })
        }
    }

    fn adapter_with(
        config: JevConfig,
        transport: Arc<dyn JevTransport>,
    ) -> JevRuleEvaluatorAdapter {
        let status = Arc::new(EngineStatus::new(EngineMode::Tier1Live));
        JevRuleEvaluatorAdapter::with_transport(config, transport, status)
    }

    fn rule(id: &str) -> RuleSpec {
        RuleManifest::embedded()
            .expect("manifest parses")
            .rule_by_id(id)
            .expect("rule exists")
            .clone()
    }

    fn evaluate(
        adapter: &JevRuleEvaluatorAdapter,
        chunks: Vec<CodeChunk>,
        rules: &[RuleSpec],
    ) -> Vec<RuleEvaluationResult> {
        crate::executor::block_on(adapter.evaluate_batch(chunks, rules))
            .expect("adapter evaluation")
    }

    #[test]
    fn default_config_matches_the_issue_budgets() {
        let config = JevConfig::default();
        assert_eq!(config.request_timeout, Duration::from_millis(2_000));
        assert_eq!(config.batch_size, 10);
        assert_eq!(config.max_parallel, 16);
        assert!(config.request_timeout <= Duration::from_millis(2_000));
    }

    #[test]
    fn http_transport_honours_the_two_second_budget() {
        let config = JevConfig {
            api_key: "sk-test".to_string(),
            ..JevConfig::default()
        };
        let transport = HttpJevTransport::new(&config).expect("client builds");
        assert_eq!(transport.request_timeout(), Duration::from_millis(2_000));
    }

    #[test]
    fn typed_answers_map_to_findings_in_rule_order() {
        let adapter = adapter_with(
            JevConfig::default(),
            Arc::new(ScriptedTransport::firing("SEC-JWT-003")),
        );
        let rules = [rule("SEC-JWT-003"), rule("SEC-XSS-004")];
        let chunk = CodeChunk::new(
            "src/auth/jwt.ts",
            "typescript",
            4,
            9,
            "const decoded = jwt.decode(token);\n",
        );

        let results = evaluate(&adapter, vec![chunk], &rules);

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].rule_id, "SEC-JWT-003");
        assert_eq!(results[0].engine, EvaluationEngine::Tier1Semantic);
        assert_eq!(results[0].findings.len(), 1);
        let finding = &results[0].findings[0];
        assert_eq!(finding.file_path, "src/auth/jwt.ts");
        assert_eq!(finding.line_start, 4);
        assert_eq!(finding.line_end, 9);
        assert_eq!(finding.severity, rule("SEC-JWT-003").severity);
        assert!((finding.confidence - 0.92).abs() < f32::EPSILON);
        assert_eq!(finding.model_probability, Some(0.92));
        assert_eq!(finding.heuristic_weight, None);
        assert_eq!(results[1].rule_id, "SEC-XSS-004");
        assert!(results[1].findings.is_empty());
    }

    #[test]
    fn answers_below_the_threshold_are_not_reported() {
        let mut answers = BTreeMap::new();
        answers.insert("SEC-JWT-003".to_string(), JevAnswer::Noul { noul: 0.2 });
        let transport = Arc::new(ScriptedTransport {
            answers: Mutex::new(answers),
            calls: AtomicUsize::new(0),
            error: None,
        });
        let adapter = adapter_with(JevConfig::default(), transport);
        let rules = [rule("SEC-JWT-003")];

        let results = evaluate(
            &adapter,
            vec![CodeChunk::new("a.ts", "typescript", 1, 1, "x")],
            &rules,
        );

        assert!(results[0].findings.is_empty());
    }

    #[test]
    fn every_chunk_is_evaluated_once() {
        let transport = Arc::new(ScriptedTransport::clean());
        let adapter = adapter_with(JevConfig::default(), transport.clone());
        let chunks = (0..25)
            .map(|index| CodeChunk::new("a.ts", "typescript", index + 1, index + 1, "x"))
            .collect();
        let rules = [rule("SEC-JWT-003")];

        evaluate(&adapter, chunks, &rules);

        assert_eq!(transport.calls.load(Ordering::Relaxed), 25);
    }

    #[test]
    fn secrets_are_redacted_before_egress() {
        let transport = Arc::new(RecordingTransport::default());
        let adapter = adapter_with(JevConfig::default(), transport.clone());
        let rules = [rule("SEC-JWT-003")];
        let chunk = CodeChunk::new(
            "src/config.ts",
            "typescript",
            1,
            1,
            "const apiKey = \"AKIAIOSFODNN7EXAMPLE\";\n",
        );

        evaluate(&adapter, vec![chunk], &rules);

        let states = transport.states.lock().expect("states lock").clone();
        assert_eq!(states.len(), 1);
        assert!(!states[0].contains("AKIAIOSFODNN7EXAMPLE"), "{}", states[0]);
        assert!(states[0].contains(REDACTED_SECRET), "{}", states[0]);
    }

    #[test]
    fn transport_failure_is_propagated_for_the_fallback_layer() {
        let adapter = adapter_with(
            JevConfig {
                max_retries: 0,
                ..JevConfig::default()
            },
            Arc::new(ScriptedTransport::failing()),
        );
        let rules = [rule("SEC-JWT-003")];
        let error = crate::executor::block_on(adapter.evaluate_batch(
            vec![CodeChunk::new("a.ts", "typescript", 1, 1, "x")],
            &rules,
        ))
        .expect_err("failing transport must surface an error");
        assert!(matches!(error, DomainError::External(_)), "got {error:?}");
    }

    #[test]
    fn retries_are_attempted_before_failing() {
        let transport = Arc::new(ScriptedTransport::failing());
        let adapter = adapter_with(
            JevConfig {
                max_retries: 2,
                ..JevConfig::default()
            },
            transport.clone(),
        );
        let rules = [rule("SEC-JWT-003")];

        let _ = crate::executor::block_on(adapter.evaluate_batch(
            vec![CodeChunk::new("a.ts", "typescript", 1, 1, "x")],
            &rules,
        ));

        assert_eq!(
            transport.calls.load(Ordering::Relaxed),
            3,
            "1 try + 2 retries"
        );
    }

    #[test]
    fn permanent_failures_are_not_retried() {
        let transport = Arc::new(ScriptedTransport::permanent());
        let adapter = adapter_with(
            JevConfig {
                max_retries: 2,
                ..JevConfig::default()
            },
            transport.clone(),
        );
        let rules = [rule("SEC-JWT-003")];

        let _ = crate::executor::block_on(adapter.evaluate_batch(
            vec![CodeChunk::new("a.ts", "typescript", 1, 1, "x")],
            &rules,
        ));

        assert_eq!(
            transport.calls.load(Ordering::Relaxed),
            1,
            "invalid-key style failures fail immediately"
        );
    }

    #[test]
    fn questions_carry_the_rule_assertion_as_instructions() {
        let transport = Arc::new(RecordingTransport::default());
        let adapter = adapter_with(JevConfig::default(), transport.clone());
        let rules = [rule("SEC-JWT-003")];

        evaluate(
            &adapter,
            vec![CodeChunk::new("a.ts", "typescript", 1, 1, "x")],
            &rules,
        );

        let questions = transport.questions.lock().expect("questions lock").clone();
        assert_eq!(questions.len(), 1);
        let assertion = serde_json::to_string(&rule("SEC-JWT-003").assertion).expect("json");
        assert!(
            questions[0].contains(&assertion),
            "instructions must embed the rule assertion: {}",
            questions[0]
        );
    }

    #[test]
    fn tier0_rules_are_never_sent_to_jev() {
        let transport = Arc::new(ScriptedTransport::clean());
        let adapter = adapter_with(JevConfig::default(), transport.clone());
        let rules = [rule("SEC-TOK-002"), rule("ARCH-DEP-001")];
        let chunk = CodeChunk::new(
            "a.ts",
            "typescript",
            1,
            1,
            "const token = \"ghp_1234567890abcdefghijABCD\";\n",
        );

        let results = evaluate(&adapter, vec![chunk], &rules);

        assert_eq!(
            transport.calls.load(Ordering::Relaxed),
            0,
            "no request sent"
        );
        assert!(results.iter().all(|result| result.findings.is_empty()));
    }

    #[test]
    fn latency_is_recorded_on_the_status_handle() {
        let status = Arc::new(EngineStatus::new(EngineMode::Tier1Live));
        let adapter = JevRuleEvaluatorAdapter::with_transport(
            JevConfig::default(),
            Arc::new(ScriptedTransport::firing("SEC-JWT-003")),
            Arc::clone(&status),
        );
        let rules = [rule("SEC-JWT-003")];

        evaluate(
            &adapter,
            vec![CodeChunk::new("a.ts", "typescript", 1, 1, "x")],
            &rules,
        );

        let snapshot = status.snapshot();
        assert_eq!(snapshot.tier1_requests, 1);
        assert!(snapshot.last_latency_ms <= snapshot.max_latency_ms);
    }

    /// Transport that records the `state` and serialized questions of every request.
    #[derive(Default)]
    struct RecordingTransport {
        states: Mutex<Vec<String>>,
        questions: Mutex<Vec<String>>,
    }

    impl JevTransport for RecordingTransport {
        fn decide(&self, request: &JevRequest) -> Result<JevResponse, JevTransportError> {
            self.states
                .lock()
                .expect("states lock")
                .push(request.state.clone());
            let questions = serde_json::to_string(&request.questions).expect("questions serialize");
            self.questions
                .lock()
                .expect("questions lock")
                .push(questions);
            Ok(JevResponse {
                answers: request
                    .questions
                    .keys()
                    .map(|id| (id.clone(), JevAnswer::Noul { noul: 0.0 }))
                    .collect(),
            })
        }
    }
}
