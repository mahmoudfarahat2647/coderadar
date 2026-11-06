# Reliable TS/JS Review — Implementation Plan

Date: 2026-11-05. Execution has not started.

**Specification:** [CR-SPEC-17 — parent issue #20](https://github.com/mahmoudfarahat2647/coderadar/issues/20).

**Plan publication:** [implementation-plan comment](https://github.com/mahmoudfarahat2647/coderadar/issues/20#issuecomment-6667638276). Planning revision: 2026-11-05, incorporating the supplied Claude review and [Antigravity review](https://github.com/mahmoudfarahat2647/coderadar/issues/20#issuecomment-6668419079).

**Goal:** Produce trustworthy, evidence-based TS/JS scan reports with optional contextual review, while remaining useful without working AI credentials.

**Architecture:** Retain the four layers and existing scan/report composition as the primary test boundary. Domain owns review policy and source identity; use cases own vendor-neutral orchestration and ports; adapters own presentation DTOs; infrastructure owns filesystem/Git/API/persistence implementations. Introduce context and reviewer ports only for capabilities absent from existing ports.

**Baseline:** Start implementation from upstream main `f121a5c52dbaa60b8c85bf94df13423e1d39f883` or its then-current successor after checking relevant changes. Its tracked tree matched local `0c89c4f58236c45c61b1d4addb2fa70ce6abf426` during planning. Preserve the two assessment notes and this planning work. Do not reset or overwrite the user's checkout.

## Confirmed testing approach

The user selected complete scans and reports as the main boundary. Tests construct temporary repositories and inject fake provider transports, then assert observable reports and permitted effects. Use lower-level tests only for policies or protocols that cannot be meaningfully covered at that boundary. Keep a small desktop integration check and separate real-provider smoke gates.

## Global constraints

- Domain: zero external dependencies. Use cases: only domain, with standard-library futures. Adapters: no dependency on infrastructure. Third-party crates remain in infrastructure.
- Workspace MSRV is Rust 1.82. Dependency/lockfile changes require the GNU Windows 1.82 toolchain check; no casual dependency upgrades.
- Preserve current patch safety, workspace boundaries, explicit cloud consent, redaction/refusal policy, no product process spawning, and signed rule installation.
- Preserve the 19-rule signed manifest and exact existing fixture unless a deliberate reviewed behavior change requires its expected set to change.
- New report contract is schema version 2; coordinated CLI/Markdown/desktop/history migration is mandatory.
- New configuration defaults: context at most 64 KiB, 8 files, 2 relationship hops; changed-code review at most 20 units; contextual output at most 20 observations and 256 KiB; all cloud work at most 100 requests per scan; contextual wall time at most 5 minutes; estimated cloud ceiling USD 0.50 subject to known conservative pricing. Unknown pricing requires explicit token/request-budget mode.
- One branch and PR per R-slice, prefixed `codex/`. Do not merge before CI passes. This plan does not authorize an implementation start or mark earlier human release gates complete.
- R1 does not depend on R0 or any working provider. Recommended serial execution order: R1, R2, R0, R3, R4, R5, R6, R7, R8. Keep existing slice IDs; R0/R1 independence permits starting with the user-visible correctness fix without creating parallel-agent work.
- The first implementation PR must commit this plan and CR-SPEC-17 as English repository documents. Use the absolute GitHub issue link until permanent repository links exist. The parent issue's slice checklist tracks merged PRs; add child issues only when execution scope is commissioned.

## Access status and dependencies

| Capability | Observed status | Required action | Work blocked |
| --- | --- | --- | --- |
| GitHub | Authenticated, issues enabled | None for spec publication | None |
| Jev/OpenRouter | Configured key rejected with 401 on metadata and synthetic inference | User creates/replaces with an authorized key; store through application keychain; rerun synthetic check | Live Jev smoke and Jev quality comparisons |
| Gemini | No CodeRadar keychain or environment credential | User creates an eligible Google AI Studio project/key and stores it through the application; check model and quota | Live Gemini smoke and Gemini quality comparisons |
| Ollama | Default local endpoint unreachable | Install/start a local runtime and compatible model if local review is desired; verify actual hardware/runtime | Live local-model smoke and quality comparisons |

Authentication rejection does not reveal whether the Jev key expired or was revoked. No key was obtained from a third party. No provider billing was activated. Offline work and hermetic provider integration are independently deliverable.

Official setup: [Google AI Studio keys](https://aistudio.google.com/api-keys), [Gemini setup](https://aistudio.google.com/docs/get-started), [Gemini pricing/data-use](https://ai.google.dev/gemini-api/docs/pricing), [OpenRouter key limits](https://openrouter.ai/docs/api-reference/limits), [Ollama model discovery](https://docs.ollama.com/api/tags). Do not promise free-tier eligibility or hardcode a model based on this dated plan; discover a compatible model, then pin the evaluated version/configuration.

## Contract map

Names below are proposed implementation contracts. Introduce them once, then use the same names across later slices.

| Contract | Owning layer | Responsibility |
| --- | --- | --- |
| ScanCoverage | Use-case boundary data | File-level partition and skip reasons returned by scanning, independent of chunk totals |
| ScanCompletion | Domain policy | Complete, partial, failed, cancelled; never inferred from finding count |
| SourceIdentity | Domain value | Snapshot identity, relative path, content hash, scan mode |
| RuleExecutionSummary | Use-case result data | Rule ID, primary owner, terminal work outcome, actual engine, fallback/incomplete counts |
| ProviderCapability | Use-case boundary data | Provider capability, status, check timestamp, sanitized reason; no secret or provider HTTP object |
| ReviewContextRequest / ReviewContextBundle | Use-case boundary data | Primary location, related source fragments, identities, limitations, bounded policy version |
| SourceContextGateway | Use-case port | Builds permitted context from a captured source snapshot |
| ContextualReviewProvider | Use-case port | Returns a parsed ReviewDraft from the supplied bundle; performs no writes or tool execution |
| ReviewDraft / ReviewDecision | Use-case result data and domain verdict policy | Model claims versus grounded supported/dismissed/insufficient-context decisions |
| ExecuteContextualReview | Use case | Schedules candidates and changed-code units, validates evidence, records limitations and budgets |
| ScanReportV2 | Outer report DTO | Versioned wire representation of scan status, coverage, provenance, evidence, and nullable score |
| SyntaxValidator | Use-case port | Validates proposed complete TS/JS content through an infrastructure parser and returns plain syntax diagnostics; no parser types cross inward |
| DiagnosticEvidenceImporter | Infrastructure capability behind a narrow use-case boundary | Bounded read-only SARIF import, source validation, provenance and deduplication |

Do not expose domain entities as API/persistence rows. Mapping stays in adapters/infrastructure. The current SemanticRuleEvaluator and GenerativeFixProvider retain their distinct jobs. Add common transport helpers only where duplication justifies them.

## Review focus

1. The working copy changes or disappears after staging: analyze the captured index, preserve safe application against the working copy (R2).
2. A provider recovers after earlier failures: retain mixed scan provenance and incomplete work; a final live badge cannot erase history (R0/R3).
3. A related source contains secrets or instructions aimed at the model: check the entire bundle and treat source as data (R5/R6).
4. A model cites valid-looking but stale lines: verify identity and exact supplied source, not just path existence (R5/R6).
5. Budget/cancellation occurs midway through a repository: stop dispatch, retain completed results, and disclose partial coverage (R6/R7).

## R0 — Provider diagnostics and offline mode

**Branch:** `codex/review-provider-capabilities`.

**Modify:** existing credential resolver, engine telemetry/composition, desktop commands, API-key dialog, engine/status components, service DTOs, CLI diagnostics handling.

**Likely files:** `crates/coderadar-infrastructure/src/{keychain,engine,composition}.rs`, `crates/coderadar-infrastructure/src/bin/coderadar.rs`, `src-tauri/src/commands.rs`, `src/components/{ApiKeyDialog,StatusBar,EngineChip}.tsx`, `src/services/tauri.ts`, `src/types/index.ts`.

**Create:** provider capability boundary/model and a narrowly scoped infrastructure diagnostics module; complete-scan provider-failure integration tests.

**Produces:** ProviderCapability plus per-scan cumulative engine usage; explicit offline baseline. Runtime capability state remains separate from scan provenance.

- [ ] Add behavioral cases for absent credentials, keychain failure, configured-but-untested access, 401, 403, credit/quota failure, 429, timeout, malformed protocol, and recovery after fallback.
- [ ] Expose a diagnostics operation that runs metadata/auth checks with no workspace content; a separate explicit synthetic inference checks actual model capability. Sanitize messages before crossing UI/report boundaries.
- [ ] Reuse current keychain-first resolution. Keep stored credentials out of diagnostics, process arguments, reports, logs, and frontend state.
- [ ] Ensure offline scanning never calls cloud transports. Do not advertise contextual review as available merely because fix generation exists.
- [ ] Preserve actual engines and failures cumulatively for each scan, independently of the latest connection status. Serialize overlapping scans or keep their telemetry isolated.
- [ ] Test a complete offline scan and a mixed failed/recovered fake-provider scan through the existing composition boundary.
- [ ] Run focused engine/CLI/security tests and frontend build. Open a PR with the observed credential status and explicit live-test limitation.

**Gate:** Offline reports are produced without keys; failed access cannot appear available; no real-provider quality claim. Live diagnostics remain open for rejected/missing credentials.

## R1 — Coverage and schema version 2

**Branch:** `codex/review-coverage-report-v2`. No dependency on R0 or provider access. This is the recommended first implementation slice.

**Modify:** scanner port/result, scanner, pipeline, scan-session summary policy, report/Markdown DTOs, SQLite history migrations, desktop scan response, status/metrics/trend UI.

**Likely files:** `crates/coderadar-usecases/src/ports.rs`, `crates/coderadar-domain/src/scan.rs`, `crates/coderadar-infrastructure/src/{scanner,pipeline,report,history,composition,export}.rs`, `crates/coderadar-adapters/src/{dto,report_markdown}.rs`, `src/types/index.ts`, `src/components/{MetricsRibbon,StatusBar,TrendChart}.tsx`, `src/utils/trendGeometry.ts`.

**Produces:** ScanCoverage, ScanCompletion, ScanReportV2, nullable compliance, readable legacy history.

**Migration decision and principal risk:** The current SQLite `compliance_percent` is `INTEGER NOT NULL`; its stored record and report/export contracts use `u8`. Change the canonical column to nullable through a transactional table rebuild, rather than pretending a DTO-only `Option` permits SQL NULL. Introduce an explicit schema migration version, preserve all rows/snapshots/constraints/indexes, and mark pre-v2 coverage unknown while retaining their numeric legacy score. New zero-coverage scans store NULL, with `Option<u8>` propagated through current persistence, presentation, and boundary summaries. No sentinel 0/100 substitutes are allowed. Old binaries that cannot read nullable rows cannot be used to downgrade-write this upgraded database.

Review R1 as two work packages: (a) scan counters/domain score policy and (b) persistence/report/presentation migration. Ship both in the same PR/release. R1 reserves unknown provider metadata rather than waiting for diagnostics; R0 fills that capability information later.

- [ ] Extend complete-scan tests for empty, Python-only, mixed TS/Python, oversized, unreadable, malformed-encoding, and excluded inputs. Assert file-level counts and that zero analyzed files yield unavailable compliance.
- [ ] Change the scanner result to carry ScanCoverage rather than deriving file coverage from module chunk totals. Preserve the streaming sink and bounded processing.
- [ ] Define documented counting precedence: explicit policy exclusion, unsupported extension, read/encoding/size eligibility failures, then analyzed/incomplete supported work. Excluded-directory contents remain unenumerated.
- [ ] Add report schema version 2 and completion/rule/provenance fields across JSON, Markdown, and desktop in the same slice. A declared optional capability's failure remains visible even when local analysis succeeds.
- [ ] Write migration behavior tests against populated databases using the actual old NOT NULL schema. Cover legacy rows with/without older optional columns, preserving IDs, timestamps, counts, provenance, scores, and findings JSON byte for byte; add new unavailable-score rows and read them back as SQL NULL.
- [ ] Apply existing predecessor migrations as needed, then rebuild the table in one SQLite transaction: create the nullable-score replacement with version/coverage metadata, copy legacy rows without inventing coverage, replace the old table, recreate required constraints/indexes, and advance the migration version only when committing. New databases start directly at the current schema. Do not swallow unexpected migration errors as if they were duplicate-column cases.
- [ ] Test idempotent reopen, persistence/upsert after migration, rollback on an injected mid-migration failure, and recovery/reopen of the untouched legacy data. Keep database opening/migration on the existing background path; a failure must leave a completed scan usable and expose the history failure.
- [ ] Propagate nullable score types through stored records, summaries, JSON/Markdown, exports, frontend types, metrics and trend geometry. Preserve old numeric scores as explicitly legacy metrics, use unknown for absent coverage/provenance, and render unavailable values as gaps instead of 100 or 0.
- [ ] Remove fabricated finding-level legacy defaults: absent engine metadata becomes unknown and absent probability/heuristic weight becomes null in history/report DTOs. Preserve stored findings JSON unchanged; explicit recorded fields remain recorded legacy values. Add round-trip/presentation cases for missing, explicit, and mixed metadata. Do not coerce absent fields back into domain certainty to satisfy an old DTO.
- [ ] Retain compliance as the documented binary file pass rate. Keep RiskScore/QualityScore separate; do not silently introduce severity weighting during this migration.
- [ ] Add round-trip tests for new reports and legacy history, CLI export, and mixed supported/unsupported scans.
- [ ] Update user-facing score labels and docs. Run scan/CLI/history/Markdown tests, trend tests, frontend build, and memory/performance gates.

**Gate:** Every surface agrees about what was analyzed. Unsupported/empty input never receives a clean certification; legacy history remains readable.

## R2 — One source snapshot for staged analysis and evidence

**Branch:** `codex/review-staged-source-snapshot`. Depends on R1.

**Modify:** Git changeset reader, scanner staged path, pipeline composition, source reader, source identity values, patch-preview source checks.

**Likely files:** `crates/coderadar-infrastructure/src/{git_changeset,scanner,pipeline,source_reader,composition}.rs`, `crates/coderadar-domain/src/{chunk,changeset}.rs`, `crates/coderadar-usecases/src/ports.rs`, `src-tauri/src/commands.rs`.

**Create:** a focused source-snapshot driver with Git-index and full-scan implementations; extend staged integration/patch-flow suites.

**Produces:** SourceIdentity and a source snapshot shared by hygiene, rule scanning, and later context lookup.

- [ ] Add the already reproduced regression: stage unsafe code, replace only the worktree with safe code, and assert identical staged findings and source hashes before/after.
- [ ] Add an explicit regression that stages unsafe content, deletes only its working-tree file, and still obtains that blob's finding/source hash. The deleted worktree must not cause `canonicalize` or existence checks to skip the indexed file. Also cover new repositories, renamed/deleted index entries, unresolved merge stages, index changes during capture, and related unchanged indexed sources.
- [ ] Capture stage-zero index entries and their blob IDs through git2 (for example, index path lookup followed by repository blob lookup). Read those blobs without rereading a mutable index during evaluation. Feed blob bytes to the same bloat/chunk/evaluation path used for supported source. Do not require the working file to exist to analyze a staged blob.
- [ ] Make hygiene use the same captured index state. Canonicalize the workspace boundary without using changing disk content as staged evidence. Record skipped conflicts/exclusions and partial capture explicitly.
- [ ] Preserve added/modified new-side line ranges from that same diff, excluding unchanged context lines. Expose changed/context-related/outside-range/unattributed scope on findings, using supported callsite/declaration evidence for context-related status. Add new/unchanged-in-same-hunk, deletion-only, rename, and unrelated-old-debt regression cases. Intersection indicates review scope, not proof of newly introduced defects. Keep PR hygiene separate; outside-range debt is visible separately, while unattributed results cannot certify a clean gate.
- [ ] For full scans, retain hashes for consumed files and invalidate results when required sources change before evidence finalization; avoid retaining all repository contents in memory.
- [ ] Keep patch generation/application bound to current working-tree hash and user-reviewed diff. Test that staged evidence cannot overwrite a divergent working copy or mutate the Git index.
- [ ] Run staged, scan, robustness, workspace-guard, and patch-flow tests plus memory gate.

**Gate:** Equal index snapshot/configuration produces equal code findings despite worktree mutations; all findings identify the actual analyzed source.

## R3 — Rule completeness, Jev responses, and probability semantics

**Branch:** `codex/review-rule-execution-contract`. Depends on R0, R1, and R2; these supply diagnostics, coverage, and source identity.

**Modify:** engine ownership, Tier 0 router, Mock/Jev adapter, pipeline execution summaries, rule/report DTOs.

**Likely files:** `crates/coderadar-infrastructure/src/{engine_rules,router,mock,pipeline,engine}.rs`, `crates/coderadar-infrastructure/src/jev/mod.rs`, `crates/coderadar-infrastructure/src/tier0/mod.rs`, focused shared modules extracted from existing file/module detectors; `crates/coderadar-domain/src/{rule,finding}.rs` where policy values belong.

**Produces:** RuleExecutionSummary, complete ownership matrix, raw probability versus heuristic-weight provenance, explicit uncertain execution.

- [ ] First add the invariant regression through full compositions: every enabled rule has exactly one primary owner and explicit fallback, and violating/safe examples for ARCH-BND-004 and PERF-HDL-002 are handled in offline, live-fake, fallback, and staged modes. Run it against the current implementation and observe the two live-path failures before changing routing.
- [ ] Extract/share `architecture_boundary` and `performance_handle` from Mock into the local evaluator implementation. Move their supporting patterns/context helpers only as needed; use one implementation per detector. Preserve detection behavior and finding locations, adapt engine-specific descriptions, tag actual engine provenance correctly, and document heuristic limitations. Avoid changing the signed manifest. Ensure no non-owned rule returns an unexplained clean result.
- [ ] Make requested answer coverage/types/ranges a transport contract. Test empty/missing/unknown/duplicate keys, unsupported answer types, out-of-range/nonfinite values, and oversized responses. Incomplete work either uses a recorded fallback or remains incomplete.
- [ ] Introduce probability/heuristic-weight distinction and uncertainty representation without silently changing the existing inclusive Mock floor or suppressed count. Keep candidates out of any claim of proven defects.
- [ ] Assert cumulative engine provenance survives failure then recovery. Add stable serialization for rule execution outcomes in report/history.
- [ ] Run Jev/router/pipeline/manifest/security tests and the original 15-finding fixture; resolve any intentional expectation changes explicitly in the PR.

**Gate:** No enabled rule silently disappears; HTTP success with unusable answers cannot be reported as a complete clean evaluation.

## R4 — Independent evaluation corpus and native baseline

**Branch:** `codex/review-quality-benchmark`. Depends on R1–R3.

**Create:** `evaluation/review-quality/` for corpus metadata/labels, a bounded benchmark runner outside product flows, and `docs/review-quality/` for generated summaries and methodology. Reuse curated fixture patterns without treating their expected set as independent quality proof.

**Produces:** versioned corpus, frozen tuning/held-out split, native baseline results, reproducible comparison schema.

- [ ] Curate at least 50 defect and 50 safe review units with provenance, expected primary/supporting locations, defect family, repository identity, reproducing evidence where practical, and approved ground truth. The project maintainer or a designated human reviewer owns label approval; record reviewer identity and approval alongside each label.
- [ ] Split by repository/family before tuning: at least 25 defects and 25 safe cases remain held out. Prevent near-duplicate tuning/held-out cases. A second human adjudicates disputed labels when available; otherwise exclude disputed cases from promotion and report them separately. Freeze a versioned split manifest before model/prompt/threshold tuning.
- [ ] Enforce that the model under test never authors, approves, or adjudicates its own labels. AI-assisted fixture preparation requires independent human approval. Without enough approved labels and the minimum held-out counts, record the evaluation gate as incomplete, even if the runner works.
- [ ] Include caller-based safeguards, sanitized sinks, handled promises, valid architecture boundaries, staged divergence, missing context, and unreviewed-file traps.
- [ ] Implement a runner that invokes complete scan compositions and records configuration/model/rule/context versions, completion, findings, runtime, and actual usage if available.
- [ ] Report precision/recall denominators, false positives per change, localization, abstention, and coverage. Undefined metrics stay undefined; abstaining on all cases does not pass.
- [ ] Run the native baseline and store raw results. Leave live-provider arms as not run with their access reasons.
- [ ] Validate repeatability and metric arithmetic using fixed small behavioral examples; keep benchmark output generated from results rather than manually edited scores.

**Gate:** A reviewer can reproduce the baseline and identify which data is reserved for evaluation. No quality improvement is claimed yet.

## R5 — Bounded repository context and evidence decisions

**Branch:** `codex/review-context-evidence`. Depends on R2–R4.

**Create:** domain evidence/verdict values; use-case `review` contracts/use case; infrastructure snapshot context builder and TS/JS symbol/import lookup; adapter evidence DTOs.

**Likely new modules:** `crates/coderadar-domain/src/review.rs`, `crates/coderadar-usecases/src/review.rs`, `crates/coderadar-infrastructure/src/review_context.rs`; extend `ports.rs`, workspace guard/source snapshot, parser/chunker integration, report presenters and exports. Split focused parsing/context modules when needed rather than growing one large driver.

**Produces:** ReviewContextRequest, ReviewContextBundle, SourceContextGateway, ReviewDraft, ReviewDecision, and citation-validation policies.

- [ ] Add complete-scan tests where a finding's apparent bug is addressed in a caller/imported helper, plus cases where an alias is unresolved or required context is omitted.
- [ ] Build a deterministic bounded index of relevant declarations, relative imports, configured aliases, direct relationships, and nearby tests. Mark dynamic/unresolved relationships explicitly; do not advertise type-aware whole-program analysis.
- [ ] Resolve context exclusively through R2 source snapshots and workspace-safe reads. Include full identities with fragments and enforce 64 KiB/8 files/2 hops while recording omitted context.
- [ ] Build a compact source-grounded import/type/declaration and architecture-layer summary for later AST windows. Attach it to evaluator requests as separate bounded/redacted context; preserve raw chunk bytes, 300-line/12-KiB caps, source line mapping, and round-trip behavior. Count summary bytes against context/request budgets and include its inputs/version in source-identity/cache tests.
- [ ] Validate returned primary/supporting references against supplied source hashes, exact excerpts, line bounds, and UTF-8 boundaries. Distinguish grounded citations from semantic correctness.
- [ ] Define candidate/supported/dismissed/insufficient-context presentation. Supported model judgments remain labeled model-supported; no votes-to-proof rule.
- [ ] Prefer the smallest evidence-supported statement/declaration span. Keep whole-chunk matches as coarse candidates until localized; prohibit using a coarse match alone to authorize a fix. Test a multi-window file with early imports/types used later and verify source references, missing-summary limitations, payload bounds, and localization metrics.
- [ ] Add tests for cycles, escaping paths/symlinks, stale files, fabricated citations, malformed ranges, truncation, and source-level prompt injection. Assert bounded outward payloads and incomplete states.
- [ ] Run domain/use-case policy tests, complete-scan tests, architecture/security and memory gates.

**Gate:** Context is bounded, snapshot-correct, and inspectable. A fake reviewer cannot inject a nonexistent or stale citation into supported findings.

## R6 — Optional contextual review through existing provider foundations

**Branch:** `codex/review-contextual-providers`. Depends on R0 and R5.

**Create:** separate Gemini/Ollama contextual-review adapters and contracts, alongside the existing fix adapters; add in-process syntax validation through a narrow port for both fix workflows. Extend desktop review configuration, CLI opt-in review composition, findings detail, provider consent, and report/history DTOs.

**Likely files:** new `crates/coderadar-infrastructure/src/contextual_review.rs` or focused provider modules; extend `composition.rs`, `keychain.rs`, `src-tauri/src/commands.rs`, `src/services/tauri.ts`, `src/types/index.ts`, `src/components/{FindingDrawer,ApiKeyDialog,ScanBar,StatusBar}.tsx`.

**Produces:** ContextualReviewProvider implementations and ExecuteContextualReview integration, preserving GenerativeFixProvider.

- [ ] Add full-scan tests with injected parsed responses showing supported, dismissed, insufficient-context, and newly nominated findings. Reject invalid evidence via R5 before presentation.
- [ ] Implement adapter prompts/parsing in infrastructure and business decisions in the use case. The inner layer does not depend on provider-specific JSON, SDKs, or names.
- [ ] Schedule Jev/static candidates plus an independent pass over supported changed units. Select at most 20 units deterministically by path/line; excess work makes contextual review partial. No Jev credential is required for this pass.
- [ ] Apply privacy and cloud consent to every bundle fragment, including tests/configuration/metadata. Refuse unsafe Gemini bundles. Validate loopback-only local claims and prevent cross-host redirects from bypassing disclosure rules.
- [ ] Enforce output caps (20 observations/256 KiB), context caps, basic request/token/time limits, and stop scheduling on cancellation in this first integration. R7 hardens accounting/reuse; it does not postpone bounded operation.
- [ ] Expose offline, Jev-assisted, and contextual-review modes with honest capability states. A no-provider scan remains useful; a requested failed review remains partial.
- [ ] Add SyntaxValidator to the use-case boundary and inject its infrastructure implementation into PreparePatch and GenerateFix. Reuse/extract grammar selection from the parser driver; never add parsing crates to use cases and never treat Chunker fallback as successful validation. Parse complete proposed content, reject error/missing nodes or unavailable dialect validation before exposing an applicable preview, and label success syntax-only. Preserve hash/path/atomic application checks.
- [ ] Add complete patch-preview cases for valid/invalid TS, TSX, JS, and JSX through both deterministic and injected-generative workflows, including originally malformed source and parseable proposals with type errors. Assert no writes during validation and type/test checks remain not run. Unsupported/parser-unavailable proposals cannot be auto-applied. Run existing patch-flow/safety tests and architecture guard.
- [ ] Run hermetic provider/scan/UI-build/security tests. For each provider with working access, run one synthetic capability request then a non-secret curated cross-file fixture, review its evidence, and record actual usage/latency.
- [ ] If credentials/runtime remain unavailable, merge only the hermetic integration with the capability labeled untested/experimental; keep the corresponding real-provider gate explicitly open.

**Gate:** Fake transports demonstrate complete behavior end to end. Each advertised live provider has a separately recorded successful smoke run; missing access is never called a pass.

## R7 — Diagnostics evidence, budgets, cancellation, and safe reuse

**Branch:** `codex/review-bounded-execution`. Depends on R5 and R6.

**Create:** bounded SARIF importer and provenance mapper; per-scan scheduling/budget ledger; scan-local review-result cache. Extend cancellation command/event flow and reports.

**Produces:** DiagnosticEvidenceImporter, cost/request/time accounting, reliable cancellation and context-aware reuse.

- [ ] Accept only user-selected/read-authorized diagnostic reports up to 10 MiB and at most 10,000 diagnostic records; reject or mark oversized import incomplete. In v1, import only source-referenced, hash-verifiable findings. Missing identities stay unverified and out of supported findings.
- [ ] Validate encoded/absolute/escaping paths, stale hashes, invalid regions, duplicate findings, and unknown tool metadata. Preserve tool provenance; do not run external analyzers, npm scripts, shells, or tests.
- [ ] Reserve remaining request and conservative estimated cost before dispatch, including allowed retries. Share the 100-cloud-request cap across Jev and generative providers; apply the 5-minute deadline to contextual review. Reconcile actual usage when supplied.
- [ ] Enforce default USD 0.50 ceiling only with conservative known prices and token/output bounds. Unknown price requires an explicit request/token-only mode and unavailable cost estimate; do not misstate a hard dollar cap.
- [ ] On budget expiry/cancellation, stop new dispatch, interrupt supported in-flight work, retain results, and mark remaining work incomplete. Verify cancellation cannot trigger late patch writes or alter the index.
- [ ] Thread a scan-scoped std-only cancellation signal through scanner, sink, scheduling, and provider workers; connect desktop abort and CLI interrupt in the outer layer. Acknowledge cancellation immediately but await tracked blocking requests up to their configured timeouts before terminal cancellation. Do not claim an AtomicBool alone aborts a blocking HTTP request; test that pending calls cannot cause late result publication or writes after termination.
- [ ] Key scan-local reuse by all source/context hashes and model/prompt/rule/threshold/privacy/mode versions. Test that changing an imported helper invalidates the primary unit's cache entry.
- [ ] Keep persistent SQLite/disk source caching deferred. A key containing only redacted chunk hash, rule ID, ruleset, and model is insufficient because imported context, prompt, thresholds, and privacy can change the result; retain the full dependency-aware key above.
- [ ] Test budget sharing under concurrent workers, retries, timeout, zero remaining budget, cancellation during context collection/provider calls, poisoned diagnostics, cross-workspace reuse, and source changes.
- [ ] Run complete-scan/provider/import tests, security/architecture guards, frontend build, and release memory/performance checks; record optional-review overhead separately.

**Gate:** Dispatch and evidence are bounded; no stale result survives a relevant context/configuration change; imports introduce no new execution privilege.

## R8 — Calibrate, compare, and make a release decision

**Branch:** `codex/review-quality-promotion`. Depends on R4–R7.

**Modify:** evaluation configuration, uncertainty/rule policy, report labels/docs, release checklist. Change signed rule definitions only through a separately authorized signed-manifest workflow if truly necessary; thresholds and routing belong to evaluated configuration.

**Produces:** versioned calibration configuration and a measured promotion/no-promotion decision.

- [ ] Use only tuning data to select per-rule probability bands, context policies, provider/model versions, and prompts. Preserve heuristic weight labeling and the Mock-floor invariants unless this slice deliberately changes and documents them.
- [ ] Freeze configuration before held-out evaluation. Run native, native+Jev, native+contextual, and combined arms where credentials permit. Record unavailable arms as not run; do not substitute fake-provider scores.
- [ ] Calculate held-out precision, recall, location correctness, false positives per reviewed change, abstention/coverage, completion/fallback rate, p50/p95 elapsed time, and actual cost with raw counts and uncertainty.
- [ ] Apply the pilot gate: at least 90% precision for model-supported findings, at least 95% correct primary location, and recall no more than 5 percentage points below the strongest measured baseline on the same units. Require nonempty defect detection and disclose completion/abstention; do not pass on selective missing data.
- [ ] If a configuration fails, retain the experimental label and record a no-promotion decision. Do not repeatedly tune against the same holdout while continuing to call it held out; obtain a new holdout for a new promotion attempt.
- [ ] Confirm report/Markdown/history/desktop agree; run complete workspace validation and appropriate release performance gates. Perform the existing human clean-machine and real-provider checks where available, retaining all unperformed gates as open.
- [ ] Update supported-language, limitations, privacy/data-use, provider setup, score semantics, budget/cancellation, and fix-validation documentation. Explain how to obtain a personal key securely without placing it in chat.
- [ ] Publish the measured decision in the PR. Promote only the configurations that passed; retain offline functionality regardless of provider selection.

**Gate:** Measured evidence justifies the product label. If live providers remain inaccessible, the offline reliability release can ship subject to its own gates, but contextual-review effectiveness remains unvalidated.

## Verification commands

Run from the repository root using the project's configured toolchain. Focus tests while implementing each slice; run all required gates before merge.

```text
cargo build --workspace --all-targets --locked
cargo test --workspace --all-targets --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
bash scripts/check-architecture.sh
bash scripts/check-security.sh
npm run build
npm run test:geometry
npm run test:trend
cargo test --release -p coderadar-infrastructure --test performance_budget --test memory_budget --locked
```

On Windows, Bash guards require Git Bash/WSL; corresponding Rust integration suites also enforce the documented invariants. After dependency/lockfile changes:

```text
cargo +1.82.0-x86_64-pc-windows-gnu check --workspace --exclude coderadar-desktop --all-targets --locked
```

Desktop performance must use the packaged production frontend workflow specified by the repository, not a bare release Cargo build loading a development URL. Optional live tests are credential-gated and record not-run reasons. Do not rerun broad checks without a relevant implementation change or unresolved failure.

## Coverage map and stop conditions

| Spec requirement / user stories | Owning slice |
| --- | --- |
| Provider access, key protection, offline capability, cumulative degradation (9–18, 32) | R0; cloud bundle policy completed in R6 |
| Coverage, scores, consistent report/history, legacy data (1–5, 36–37) | R1 |
| Staged source identity, working-copy divergence (6–7) | R2 |
| Rule execution, protocol completeness, uncertainty provenance (8, 29, 31) | R3 |
| Independent labels and repeatable evaluation (40–41) | R4, R8 |
| Context, missing information, grounded evidence (19–27) | R5, R6 |
| Independent changed-code review and optional providers (17–18, 28) | R6 |
| Budget, cancellation, caching, external diagnostics (33–35, 42) | R7 |
| Safe proposals and honest validation status (38–39) | R2, R6, R7; existing safe writer retained |
| Calibrated thresholds and measured promotion (30, 40–41) | R8 |

Do not report live Jev, Gemini, or Ollama quality as verified until its access and real smoke/evaluation gates pass. Do not let those access gaps block R0–R5 and hermetic work. Stop before changing billing, weakening architectural boundaries, adding process execution, or bypassing manifest signatures; those actions are outside this plan.

## Planning confidence and expected outcome

R0–R3 repair independently observable correctness defects. R4 makes the quality question measurable. R5–R7 add the missing context and review workflow. R8 determines whether those additions are effective; the expected outcome is a defensible measurement, which may be a no-promotion decision for a provider/configuration.

No calendar commitment is implied. R5 context resolution, history/report migration, and R8 corpus labeling are the largest uncertainty areas. Implementation should size each PR after inspecting the then-current source; split a slice only when each part has its own meaningful gate, while preserving the dependency order and contracts above.

## Self-review

- The 42 user stories map to named slices; every review-focus condition has a regression owner.
- Report schema, source identity, context/evidence, and provider contracts are introduced before consumers.
- Provider availability is observed, not inferred from configured keys. Current access gaps remain explicit.
- The plan preserves signed-manifest, patch-safety, MSRV, no-process-spawn, and dependency constraints.
- Product budgets and pilot quality targets are proposed requirements, not measured performance or accuracy claims.
- No application code was changed to produce this plan; temporary access probes are outside the repository.
