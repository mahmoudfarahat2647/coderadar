# CodeRadar review effectiveness assessment

Date: 2026-11-05. Scope: the active Rust/Tauri/React application, not vendored review-skill documents. This is an assessment, not an implementation change.

## Verdict

CodeRadar has a real, tested application foundation: local scanning, engine routing, streaming results, patch conflict checks, history, reports, and signed rules. Its current detection scope and evidence quality do not support a claim that it reliably reviews arbitrary projects. Adding an API alone would not close that gap.

Keep Jev as a candidate classifier if evaluation shows it improves cost, latency, or accuracy. Add contextual review/verification, initially through an existing generative provider, and measure it against a static-only baseline. Do not make Jev the sole gate for deeper review: its false negatives would prevent the verifier from seeing bugs.

## Validation performed

- `cargo test --workspace --all-targets --locked`: passed, including architecture/security tests and debug performance/memory checks.
- `npm run build`: passed.
- Two additional black-box experiments ran the built CLI on disposable synthetic projects outside the repository; results below.
- Source inspection covered scanning, chunking, routing, Jev requests/responses, fallback, scores, Tier 2 generation, desktop wiring, and fixture tests.
- No authenticated live Jev/Gemini/Ollama calls were made. Current credentials, provider availability, real-model accuracy, installed desktop behavior, and release performance were not tested.
- Passing tests demonstrate the behaviors they assert; they do not establish real-world review precision or recall.

## Confirmed problems and limitations

### P0: An unsupported project receives 100% compliance

Reproduction: create a Python-only directory containing `subprocess.run(user_input, shell=True)` and run `coderadar scan <directory>`.

Observed: exit code 0, `files_scanned: 0`, no findings, `compliance_percent: 100`, `engine_mode: mock`. This is not evidence that the Python code is safe; Python is not analyzed at all.

Evidence: `crates/coderadar-infrastructure/src/chunker.rs:33` supports only TS/JS extensions; `crates/coderadar-domain/src/scan.rs:139` explicitly returns 100 for zero scanned files.

Required outcome: distinguish unsupported, skipped, incomplete, failed, and analyzed-clean results. Show supported coverage and reasons for skipped files. Zero coverage should produce an unavailable score. Even for supported files, describe the metric as the share of analyzed files without reported findings, not a general security certification.

### P0: Staged code analysis reads working-tree content

Reproduction in a disposable Git repository:

1. Commit a simple `auth.ts`.
2. Put `export const value = jwt.decode(token);` in it and stage the file.
3. Run `coderadar scan <directory> --staged`: `SEC-JWT-003` is present.
4. Change the working copy to `export const value = 1;` without staging again.
5. Repeat the same scan: `SEC-JWT-003` disappears, while the staged index is unchanged.

The hygiene portion reads the staged diff, but source analysis selects staged paths and reads their current disk content. The project instruction that staged scanning reads only the index is therefore not fulfilled by this code path.

Evidence: `crates/coderadar-infrastructure/src/scanner.rs:124` calls `stream_file`, which reads disk bytes at line 192.

Required outcome: read and analyze the staged blob, tie results to its identity, and add a regression case where staged and unstaged contents differ. Keep the user's Git index untouched.

### P1: Two declared rules have no evaluator on the successful live path

Source-traced defect: `ARCH-BND-004` and `PERF-HDL-002` are declared `tier0` in `rules/manifest.json`. The router delegates only three hardcoded IDs to Tier 0; these two go to the semantic engine. Mock implements them, but Jev's `is_semantic_rule` rejects every manifest rule marked Tier 0. Jev returns empty results without a request for those rules, so fallback is not triggered.

Evidence: `crates/coderadar-infrastructure/src/engine_rules.rs:14`, `:25`; `src/router.rs:47`, `:121`; `src/jev/mod.rs:425`; `src/mock.rs:581`, `:587` (paths after the first refer to the same crate).

Required outcome: explicit ownership for every enabled rule and a coverage test across mock/live/fallback compositions. Do not merely change the signed manifest without implementing the correct evaluator and respecting signature requirements.

### P1: Jev findings lack localized evidence

The adapter sends one chunk with file/language/kind/line metadata and rule assertions. It does not retrieve callers, callees, imported definitions, project policy, tests, or a dependency graph. It emits the whole chunk's line range with a generic assertion-based description. A probability is useful triage, but is not a source-grounded explanation of a bug.

Evidence: `crates/coderadar-infrastructure/src/jev/mod.rs:299`, `:451`, `:480`.

Required outcome: candidate findings become verified findings only after checking precise source locations, relevant context, the triggering scenario, and why existing safeguards do not address it. Preserve uncertain candidates as a separate state.

### P1: Uncertainty is treated as a positive finding

The current default emits a finding whenever `noul >= 0.5`. TypeSafe documents a Noul near 0.5 as uncertainty and suggests a distinct intermediate outcome; its suggested 0.3/0.7 bands are starting points, not a validated code-review threshold. Mock values such as 0.55/0.75 are hardcoded heuristic weights, not measured probabilities of correctness.

Evidence: `crates/coderadar-infrastructure/src/jev/mod.rs:54`, `:352`; `src/mock.rs` trigger matrix; [TypeSafe confidence documentation](https://docs.typesafe.ai/confidence).

Required outcome: tune decisions per rule on labeled cases, distinguish provider probability from heuristic weights, and support abstention/context gathering. Raising a single global threshold without measuring recall is insufficient.

### P1: Transport success can be mistaken for completed evaluation

`JevResponse.answers` defaults to an empty map. Missing answers and unknown answer variants can become no findings rather than an incomplete evaluation. This is a source-inspected robustness gap, not an observed live-service incident.

Evidence: `crates/coderadar-infrastructure/src/jev/mod.rs:123`, `:142`, `:347`.

Required outcome: validate answer coverage, expected types, and probability range. Missing or invalid responses should be explicitly incomplete/retried/degraded, never treated as evidence of cleanliness.

### Product scope and quality evidence

- The embedded manifest contains 19 rule definitions, including three staged-diff hygiene rules. This is a limited rule catalog, not general repository understanding.
- TS/JS parsing supplies chunk boundaries; it is not a type checker or interprocedural data-flow engine.
- The CLI compositions deliberately use Mock. Desktop Jev selection is real, but a CLI scan does not establish Jev quality (`composition.rs:37`).
- The exact 15-finding fixture is tested through the offline `scan_project` composition (`tests/scan_pipeline.rs:57`). There are additional unit and clean-pattern tests, but no broad labeled real-project evaluation was found.
- Gemini and Ollama already implement `GenerativeFixProvider`; they take an existing finding and one file to propose a replacement. They do not independently review a repository (`src/tier2.rs`, `crates/coderadar-usecases/src/tier2.rs`).
- Current patch guards protect file integrity, path boundaries, and stale content. They do not prove a proposed fix compiles, passes tests, or preserves behavior.

## Recommended delivery order and acceptance criteria

1. **Trust and correctness first:** repair zero-coverage scoring, staged blob reading, missing rule ownership, and incomplete Jev responses. Regression tests must cover each observed/source-traced case. Keep per-scan coverage and degradation separate from the current connection status.
2. **Choose a bounded launch scope:** reliable TS/JS review first. Explicitly report other languages as unsupported; add language packs gradually with their own evaluation sets.
3. **Build a review-quality benchmark:** start with 100-200 independently labeled positive and negative examples, then real held-out repositories/PRs. Include cross-file behavior, safe lookalikes, changed-line attribution, missing context, and unsupported inputs. Measure precision, recall, false positives per PR, localization, user acceptance, cost, and end-to-end latency by rule. Suggested release targets must be agreed and measured, not inferred from a green test suite.
4. **Strengthen deterministic evidence:** use type/lint diagnostics, security/data-flow tooling, dependency vulnerability information, and secret scanning where applicable. Evaluate importing structured results (for example SARIF) first. The current product forbids process spawning, so embedding executable tools would require an explicit reviewed architecture/security change; do not silently bypass that guard.
5. **Add repository context and review verification:** resolve imports and symbols, collect relevant definitions/call sites/tests and configured architecture boundaries, and prioritize changed code. Add a new inner-layer review port with simple DTOs; implement model/tool adapters in infrastructure. Extend the existing Gemini/Ollama integration through that port rather than conflating review and patch generation.
6. **Control cost and prove fixes:** use changed-file/content-hash caching keyed by model/rule/context versions, bounded scheduling, rate-limit-aware retry/backoff, cancellation and cost visibility. Validate proposed fixes with syntax/type checks and relevant tests in a controlled validation path before calling them verified. A generated diff still needs review.

## API decision

Jev is suitable for structured classification/routing/scoring. Its official docs explicitly say it does not generate text or code and is not a coding-agent LLM: [Jev with coding agents](https://docs.typesafe.ai/introduction/coding-agents).

For contextual explanations, novel bug hypotheses, and multi-file review, a generative reviewer is a reasonable addition, but it must receive retrieved context and evidence checks. The repository already contains two provider integrations that can be extended; a new vendor or paid subscription is not the first missing dependency. Compare static-only, static-plus-Jev, static-plus-generative-review, and combined pipelines on the same held-out set before deciding whether Jev earns its place.

Historical repository latency measurements are approximately 0.5-0.6 seconds warm in the Phase 0 note and 908 ms mean in the later 19-request Phase 5 run. These were not rerun during this assessment and are not current SLA guarantees. Fast classification does not by itself establish useful review accuracy.

See [the companion primary-source API research](jev-api-research-2026-11-05.md) for current documentation, pricing/latency caveats, and supporting links.
