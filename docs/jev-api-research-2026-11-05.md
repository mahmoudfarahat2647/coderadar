# Jev API research for CodeRadar

Research date: 2026-11-05. This note separates current first-party documentation, historical measurements recorded by this repository, and engineering recommendations. No authenticated API calls were made and no credentials were accessed.

## Finding

Jev is real and the integration's endpoint/model alias match OpenRouter's documentation. It can contribute useful bounded judgments to a review pipeline. It does not supply the explanations, code edits, repository navigation, or tool execution of a coding agent. Adding another API alone would not establish review quality; context, evidence, evaluation data, and uncertainty handling are the larger missing capabilities to address.

## Verified API facts

| Topic | Verified finding | Primary source |
| --- | --- | --- |
| Identity and output | TypeSafe describes Jev as a System One decision model. It returns choices, scores, and yes/no probabilities; it does not generate explanations or code and cannot replace a coding-agent LLM. | [TypeSafe: Jev with coding agents](https://docs.typesafe.ai/introduction/coding-agents) |
| Direct API | `POST https://api.typesafe.ai/v1/systemone` accepts a model, state, and named questions. Answers match question names and types. `noul` is a probability of yes/true; values around 0.5 express uncertainty. | [TypeSafe API reference](https://docs.typesafe.ai/api/system-one) |
| OpenRouter integration | OpenRouter documents `POST https://openrouter.ai/api/alpha/decisions`, model `typesafe/jev-1.14`, and alias `~typesafe/jev-latest`. Its alternate `/api/v1/systemone` surface supports TypeSafe SDK clients. Both use an OpenRouter key. | [OpenRouter Jev guide](https://openrouter.ai/docs/guides/community/jev) |
| Current advertised cost | The Jev 1.14 model page lists $0.042 per million input tokens and free output. Its displayed provider p50 latency was 0.10 seconds at research time; that is provider telemetry, not a guarantee for CodeRadar or this network. | [OpenRouter model page](https://openrouter.ai/typesafe/jev-1.14) |
| Context and versions | TypeSafe lists default limits of 64k tokens per request and 32k per question. Its explicit long-context mode raises both to 262k but is slower and less accurate. Model aliases move; TypeSafe advises version pinning after threshold tuning. | [TypeSafe models](https://docs.typesafe.ai/models) |
| Rate limits | TypeSafe's current limits are organization-specific, measured in requests and tokens per second. Its SDK retries with backoff and respects `retry-after`. A fixed universal limit should not be inferred from older project assumptions. | [TypeSafe models](https://docs.typesafe.ai/models) |

Some search results lead to similarly named independent gateways. They are not interchangeable with TypeSafe's own service. For this project, use TypeSafe and OpenRouter documentation as the authority for their respective endpoints, authentication, and prices.

## Implications for review quality

**Uncertainty must remain visible.** TypeSafe distinguishes the `noul` probability from the separate confidence statistic used by Choice/Score. It suggests 0.3/0.7 as starting Noul bands, with an explicit uncertain outcome between them; thresholds must be tested on the application's own data and adjusted by consequences. These are starting points, not a recommended universal CodeRadar configuration. [TypeSafe confidence](https://docs.typesafe.ai/confidence)

CodeRadar currently sets `DEFAULT_CONFIDENCE_THRESHOLD` to 0.5 and emits answers at or above it. This turns an uncertain answer into a finding. The adapter also formats the raw Noul value as a confidence percentage. Recommendation: preserve raw probability, introduce an abstain/review state, and fit separate thresholds for each rule using held-out positive and negative examples. This is an engineering recommendation, not a measured improvement. See [Jev adapter](../crates/coderadar-infrastructure/src/jev/mod.rs).

**Supply related context deliberately.** All questions in a request see one state and are evaluated independently. TypeSafe recommends structured state when relationships matter, grouping the relevant facts together. [TypeSafe state](https://docs.typesafe.ai/concepts/state)

The current adapter supplies one chunk's path, language, kind, line range, and content. Its findings cover that chunk's entire range and use a generic description made from the assertion. It does not obtain a precise supporting line or written reasoning from Jev. Recommendation: construct evidence candidates using parsers, imports, callers, configuration, tests, and changesets; ask bounded questions about those candidates. A larger context window alone does not construct this context. See [Jev adapter](../crates/coderadar-infrastructure/src/jev/mod.rs).

**Use a generative model for a distinct task.** A model that can read selected supporting files and produce an evidence-backed explanation or proposed fix complements Jev. Treat generated explanations as claims to verify, not as Jev's hidden reasoning. The project already contains Ollama and Gemini fix providers, so it has a starting integration rather than needing a new vendor by default. See [existing fix providers](../crates/coderadar-infrastructure/src/tier2.rs). This recommendation follows from Jev's documented output limits, not a benchmark showing that a particular generative model is best. [TypeSafe coding-agent guidance](https://docs.typesafe.ai/introduction/coding-agents)

**Add deterministic evidence.** Semgrep Code supports local and CI static security scans. CodeQL's data-flow analysis follows possible values and unsafe data use through a program. These are complementary capabilities worth evaluating for adapters rather than rebuilding every detector as a question. This is not a claim that either tool detects every issue, or that all capabilities have identical licensing. [Semgrep overview](https://docs.semgrep.dev/semgrep-code/overview), [CodeQL data flow](https://codeql.github.com/docs/writing-codeql-queries/about-data-flow-analysis/)

## What the existing measurements establish

The repository records approximately 0.49-0.57 seconds per warm request in its October 16 spike and a mean of 908 ms across 19 chunk requests on October 17, with no fallback in that run. Those are historical project measurements, not fresh measurements from this assessment. They support the fact that the integration was exercised, not a claim about present availability or review precision. See [Phase 0 spike](../coderadar-specs/08-PHASE-0-SPIKE-AND-DECISIONS.md) and [Phase 5 evidence](../coderadar-specs/12-PHASE-5-JEV-TIER1-AND-PRIVACY-LAYER.md).

The public pricing agrees with the historical input rate, but current provider latency and a local end-to-end scan measure different things. Re-measure requests, whole-scan duration, retry/fallback rates, and actual billed input before making a new performance promise. The sources reviewed do not establish Jev's precision or recall for CodeRadar's code-review rules.

## Recommended decision

Keep Jev as a candidate classifier/ranker or verifier, subject to rule-specific evaluation. Build a repeatable benchmark of real defects and safe lookalikes; compare static-only, Jev-assisted, and generative-assisted review on precision, recall, location accuracy, review usefulness, cost, and latency. Preserve a smaller sample for tuning and a separate held-out set for the decision. Only choose an additional provider once that benchmark shows the capability and cost tradeoff needed.

For broad repository review, combine local deterministic checks, repository context retrieval, optional generative analysis, and evidence verification. Keep adapters in infrastructure and stable review policies behind inward-facing ports. Jev can save time and cost inside this design, but its speed is not evidence that a finding is correct.
