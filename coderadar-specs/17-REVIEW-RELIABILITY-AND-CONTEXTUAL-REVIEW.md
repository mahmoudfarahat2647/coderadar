# CR-SPEC-17 — Reliable, Evidence-Based TS/JS Review

Date: 2026-11-05. Status: ready for implementation planning; production behavior is unchanged.

Planning revision: 2026-11-05, incorporating the supplied Claude review and [Antigravity review](https://github.com/mahmoudfarahat2647/coderadar/issues/20#issuecomment-6668419079). Tracking: [parent issue #20](https://github.com/mahmoudfarahat2647/coderadar/issues/20); [detailed plan](https://github.com/mahmoudfarahat2647/coderadar/issues/20#issuecomment-6667638276).

Repository: https://github.com/mahmoudfarahat2647/coderadar

Baseline: local commit `0c89c4f58236c45c61b1d4addb2fa70ce6abf426`; upstream main `f121a5c52dbaa60b8c85bf94df13423e1d39f883`. A fetch and tree comparison confirmed identical tracked content despite the two merge commits on main. Implement from current main, using one branch and PR per delivery slice.

## Problem Statement

A developer wants CodeRadar to identify actionable problems in another project and explain why each reported problem matters. Today a successful scan can conceal unsupported files, missing rule execution, uncertainty, or unavailable providers. A Python-only project receives 100% compliance despite zero analyzed files. Staged scans select staged paths but analyze working-tree bytes. Two declared rules disappear on the successful Jev path. Jev findings cover entire chunks and repeat generic assertions rather than providing localized evidence and relevant repository context.

The application has a real Rust/Tauri/React foundation, safe patching, history, and tests. However, those implementation tests do not establish real-world review precision or recall. The user does not currently have usable generative review access, and the stored Jev credential fails authentication. Buying or attaching another API does not by itself establish reliable review.

## Solution

Deliver reliable TypeScript/JavaScript review in independently shippable increments. First make scan coverage, source identity, rule execution, provider availability, and incomplete results truthful. Then supply bounded repository context and evidence to optional contextual reviewers. Keep Jev as an optional structured classifier whose value must be measured. Reuse Gemini and local Ollama integration foundations through a separate review capability; retain the existing fix-generation workflow.

Users can run an offline baseline immediately. Cloud review is opt-in and requires a working user-owned credential. Each result identifies its source, exact location, evidence, review state, and limitations. No model's probability or agreement with another model constitutes proof of correctness. Unsupported, incomplete, uncertain, and clean-within-coverage are distinct outcomes.

## User Stories

1. As a developer, I want to see which languages are supported, so that I know whether the scan applies to my project.
2. As a developer, I want unsupported files counted separately, so that an unexamined project cannot appear clean.
3. As a developer, I want skipped-file reasons, so that I can understand gaps caused by size, encoding, access, or policy.
4. As a developer, I want a zero-coverage score to be unavailable, so that I do not mistake absence of analysis for compliance.
5. As a developer, I want a score explained as a metric over analyzed files, so that I understand its limited meaning.
6. As a developer, I want staged scans to inspect the exact Git index content, so that results describe the next commit.
7. As a developer, I want working-tree changes to leave staged results unchanged, so that unrelated edits cannot hide staged problems.
8. As a developer, I want every enabled rule to report its execution state, so that silent rule gaps are visible.
9. As a developer, I want offline scanning without credentials, so that I can use the application immediately.
10. As a developer, I want local heuristic findings identified honestly, so that I can distinguish them from model judgments and deterministic evidence.
11. As a developer, I want provider status checked without uploading my project, so that I can diagnose access safely.
12. As a developer, I want missing, rejected, quota-blocked, rate-limited, unavailable, and untested providers distinguished, so that I know what action is needed.
13. As a developer, I want a credential's presence separated from its validity, so that a configured key does not imply working access.
14. As a developer, I want my API keys kept in the operating-system keychain, so that reports, logs, and browser state do not expose them.
15. As a developer, I want explicit consent before cloud review, so that I control source disclosure.
16. As a developer, I want cloud privacy checks applied to every context fragment, so that related files cannot bypass the source-file safeguards.
17. As a developer, I want local review when an installed Ollama model is available, so that cloud credentials are optional.
18. As a developer, I want unavailable review capabilities stated explicitly, so that offline results are not presented as completed AI review.
19. As a developer, I want each suspicious location associated with relevant definitions and callers, so that review considers existing safeguards.
20. As a developer, I want relevant tests and project configuration included, so that review can distinguish intended behavior from defects.
21. As a developer, I want unresolved imports and truncated context recorded, so that reviewers can abstain when information is missing.
22. As a developer, I want precise primary and supporting locations, so that I can inspect the evidence quickly.
23. As a developer, I want a concrete triggering scenario and impact, so that each finding explains a practical problem.
24. As a developer, I want uncertain candidates separated from supported findings, so that the main report remains useful.
25. As a developer, I want dismissed candidates retained as a bounded audit summary, so that I can understand review decisions without noise.
26. As a developer, I want model-returned citations checked against the analyzed snapshot, so that invented files and lines cannot appear as evidence.
27. As a developer, I want contradictory model opinions to remain unresolved until evidence supports a conclusion, so that agreement is not substituted for validation.
28. As a developer, I want important changed code reviewed independently of Jev nominations, so that Jev false negatives do not block all deeper review.
29. As a developer, I want Jev probability distinguished from heuristic weights, so that displayed percentages have an honest meaning.
30. As a maintainer, I want per-rule thresholds calibrated on labeled cases, so that reducing noise does not silently destroy recall.
31. As a developer, I want broken or incomplete provider responses reported as incomplete, so that a transport success cannot become a false clean result.
32. As a developer, I want mixed live and fallback execution recorded throughout a scan, so that later recovery cannot erase earlier degradation.
33. As a developer, I want request, context, time, and cost limits, so that review cannot spend without bounds.
34. As a developer, I want cancellation to preserve completed results and stop new work, so that I control long scans.
35. As a developer, I want unchanged work reused only when its context and configuration still match, so that caching remains correct.
36. As a developer, I want scan reports, history, and the desktop to display the same coverage and review states, so that results remain consistent across surfaces.
37. As a developer, I want older history labeled with unknown coverage metadata, so that migration does not invent evidence.
38. As a developer, I want fix proposals kept separate from review decisions, so that a proposed rewrite is not mistaken for a validated correction.
39. As a developer, I want validation status attached to fixes, so that I know whether only content integrity or actual program behavior was checked.
40. As a maintainer, I want held-out defects and safe lookalikes in evaluation, so that quality is measured beyond handcrafted regression fixtures.
41. As a maintainer, I want accuracy, localization, coverage, latency, and cost compared across provider modes, so that architecture decisions follow evidence.
42. As a developer, I want the application to report when trusted evidence ingestion is unavailable, so that a proposed static-tool integration is not represented as an existing engine.

## Implementation Decisions

### 1. Invariants and delivery boundaries

- Launch scope is TS/JS, including the extensions already recognized by the scanner. Additional language packs are deferred.
- Preserve the Dependency Rule: domain has zero dependencies; use cases depend only on domain; adapters do not depend on infrastructure; external libraries remain in infrastructure. Use standard-library future trait objects for asynchronous inner ports.
- Inner policies use vendor-neutral concepts and plain boundary data. They do not mention provider SDKs, HTTP envelopes, SQLite rows, UI types, or outer-layer declarations. Presenters translate domain results to DTOs; entities do not cross external boundaries directly.
- Preserve MSRV 1.82 for the workspace and all existing architecture, security, patch, signing, and performance guards. Desktop has its existing separate toolchain requirements.
- Do not introduce product process spawning or execute repository scripts. Additional command-execution validation is a separate architecture/security decision.
- Preserve the signed 19-rule manifest. Repair evaluator ownership in code; do not edit signatures or add/remove rules as a shortcut. Future catalog changes require the signing key and coordinated count updates.
- One independently testable delivery slice, branch, and PR at a time; merge only after its applicable gate passes. Earlier release/install and live-provider gates remain recorded as open where appropriate.

### 2. Truthful coverage and report contract

- Introduce report schema version 2. Keep completion, coverage, rule execution, and provider capability as separate dimensions.
- Completion is complete, partial, failed, or cancelled. Complete means the selected supported capabilities finished, not that all repository behavior is proven correct.
- Record files discovered, excluded by declared policy, supported/eligible, analyzed, unsupported, unreadable, and skipped by reason. Define a consistent partition and count each file once at file level, regardless of AST chunk count. Do not traverse excluded directories solely to count their contents; report those exclusions as policy scope.
- A file is analyzed only when its planned rule work reaches terminal recorded outcomes. Errors, missing answers, or exhausted budgets cannot masquerade as clean evaluation. Explicitly disabling an optional capability changes declared scope; an enabled capability failing makes that work incomplete.
- Compliance is nullable when zero supported files were analyzed. For nonzero coverage retain the current file-pass calculation over analyzed files and label it as the share without reported findings. Pair it with completion and coverage limitations; never call it a security certification.
- Version and migrate CLI JSON, Markdown, desktop DTOs, and persisted history together. Keep old rows readable; old coverage and provenance metadata remain unknown. Existing stored numeric scores may be displayed as legacy metrics, never reinterpreted as newly measured coverage. Capture actual engines used per scan; preserve the legacy engine-mode token only as a compatibility field.
- Finding-level legacy provenance is also unknown when absent: missing engine fields must not default to Tier 0, and missing probability/weight fields must not default to 1.0. Preserve original stored JSON and explicit recorded values; distinguish unavailable provenance from recorded-but-legacy metadata. History DTOs display unknown/null without inventing domain confidence or deterministic evidence.
- Compliance remains a binary pass rate over analyzed files. RiskScore/QualityScore retain their separate meanings; introducing a severity-weighted session score is outside this release and must not silently redefine existing history.
- Storage decision: the current SQLite score column is NOT NULL. R1 must rebuild the history table inside a single transaction so that the canonical score column becomes nullable, copying all existing rows and findings snapshots without loss. Preserve legacy numeric scores and mark their coverage unknown; new zero-coverage rows store SQL NULL. Version the migration, make reopening idempotent, preserve constraints/indexes, and roll back fully on failure. Propagate nullable scores through persistence and every report/presentation contract. History remains background work; migration failure is reported without invalidating a completed scan. Downgrading to an older binary that cannot read nullable scores is not supported.
- R1 has no provider dependency. It can deliver coverage and schema correctness before R0; reserve unavailable/unknown provider metadata until diagnostics is integrated. Within R1, review scanner/score changes and persistence/presentation changes as two work packages, but release them together so no surface temporarily misrepresents NULL as 0 or 100.

### 3. Source identity and staged correctness

- A staged scan reads file contents and related context from a captured Git index snapshot. Hygiene and code analysis share that snapshot. Resolve blobs through the existing Git library, including added/renamed files, repositories without commits, conflicts, and deleted entries.
- Explicit regression: a file present in the index but deleted only from the working tree must still be analyzed from its captured blob. A filesystem existence or canonicalization check must not cause it to disappear from staged analysis.
- Staged results remain unchanged if only the working tree changes. Detect index changes during capture; either finish against the captured snapshot or report a restartable incomplete capture. Never combine two index states.
- Related context for staged review comes from the index snapshot, including unchanged indexed files, rather than disk. Unsupported or excluded files remain explicitly outside analysis scope.
- Preserve exact added/modified new-side line ranges from the captured diff, excluding unchanged hunk context. Tag code findings as directly changed, context-related, outside changed ranges, or unattributed. Deletions-only changes attach review to an affected declaration/file when a surviving primary line cannot be cited. Never infer that a finding is newly introduced solely from line intersection; proving introduced-versus-existing requires a comparable base evaluation. By default, PR-blocking code findings require grounded directly changed or context-related evidence; show outside-range debt separately without hiding it. Existing PR-hygiene rules retain their changeset-level policy. Unattributed results cannot certify a clean PR gate.
- For full scans, retain hashes for consumed content and verify them before reusing or presenting evidence. If a required file changes during review, invalidate affected work or record it as partial; do not silently claim a coherent snapshot.
- Source identities include scan mode and content fingerprints. Applying a fix still checks the current working-tree hash through the existing safe-patch workflow; staged analysis never authorizes overwriting a divergent working copy or modifying the index.

### 4. Rule ownership, protocol coverage, and uncertainty

- Every enabled rule has one explicit primary owner for its scope and an explicit fallback policy. Validate the ownership matrix before scanning. Implement the two missing file/module rules locally and characterize their limitations as static heuristics where applicable.
- Reuse the existing Mock architecture-boundary and handle-release detectors by extracting/sharing their implementation in the local rule engine. Add the cross-composition ownership and violating-fixture regression tests first. Preserve their detection behavior while changing ownership; adapt engine-specific descriptions so a local static heuristic is not mislabeled as Mock or semantically proven. Do not maintain duplicate detector implementations.
- Keep static heuristics, deterministic checks, and model judgments distinct in provenance. Being local or tagged Tier 0 does not establish semantic certainty.
- Validate Jev answer coverage against requested rule IDs, answer types, finite probabilities in range, and supported protocol shape. Missing/unknown/invalid answers produce incomplete work or an explicit fallback, never an empty clean result. Reject ambiguous duplicate answer keys during parsing.
- Preserve the raw Noul probability separately from heuristic weights. Probability near 0.5 is uncertainty. Use per-rule reject/review/support bands fitted on labeled data; do not replace the existing threshold with an untested universal 0.9 rule.
- Changing the existing inclusive Mock confidence floor is outside the first repair slice. Its current behavior and suppressed-count invariant remain pinned until the calibrated policy is introduced through an explicit, tested change.
- Freeze provider/model, prompt, threshold, and context-policy versions for an evaluated configuration. A moving model alias invalidates calibration unless the resolved version is recorded and re-evaluated.

### 5. Contextual review capability

- Add one vendor-neutral contextual review use case as the public business operation. Reuse the scanner, changeset, history, and semantic evaluator ports where their contracts fit. Use narrow source-context and contextual-review provider ports only where new capabilities require them; do not create a port per rule.
- Keep review separate from GenerativeFixProvider. Reuse transport/credential/privacy foundations in infrastructure while maintaining different request and response contracts.
- Context construction gathers the primary function/module, resolved relevant imports, bounded direct callers/callees, nearby tests, architecture configuration, and changed ranges. First release supports relative imports and configured local aliases; unresolved/dynamic relationships are explicit limitations. Do not claim a complete call graph or general type/data-flow analysis.
- Supply later AST windows with a compact, source-grounded import/type/declaration summary and resolved architecture layer as separate request context. Do not prepend synthetic text to the raw CodeChunk or change its source offsets/round-trip contract. Retain the 300-line/12-KiB raw source cap and count the summary toward the separate context/request budgets; redact all outbound summary content. Its source identities and policy version participate in cache invalidation.
- Default maximum context bundle: 64 KiB UTF-8 text across at most 8 files, with at most 2 relationship hops; these are configurable engineering budgets, not claims about model capacity. Record omitted relationships and truncation. The bounded file index is separate from retained source text and must respect memory gates.
- Review both nominated candidates and supported changed functions independently. Initial changed-code cap is 20 function/module units per scan, processed in deterministic path/line order. If eligible work exceeds the cap, mark that review stage partial and report the omitted count. Users may adjust the budget explicitly.
- Full-project contextual review is opt-in and budgeted. It must never appear complete if only a subset was reviewed. Jev is optional in every contextual mode.
- Reviewer output contains a verdict, primary source reference, bounded supporting references, triggering scenario, impact, rationale, limitations, and optional remediation guidance. Allow at most 20 returned observations per request and at most 256 KiB response text before parsing; reject excess output as a protocol/budget failure.
- Verdicts are supported, dismissed, or insufficient context. New nominations remain candidates until the same evidence checks run. A supported model judgment is labeled model-supported, not independently proven.
- Validate cited paths, hashes, line/UTF-8 boundaries, and excerpt identity against supplied snapshot context. A failed citation cannot reach the supported findings feed. Correct citations validate grounding, not the truth of the model's reasoning.
- Prefer the smallest supported statement/declaration span. A whole-chunk semantic match with no localized evidence remains a coarse candidate, cannot justify automatic remediation, and does not count as an accurately localized supported finding in evaluation.
- Repository text is untrusted data and cannot change review instructions, choose arbitrary endpoints, execute tools, or read files outside the authorized workspace. All source access uses workspace-boundary protections.

### 6. Capability discovery, credentials, and provider failure

- Capability states distinguish not configured, configured but untested, available, unauthorized, forbidden, quota/budget blocked, rate limited, unreachable, and protocol incompatible. Availability has a timestamp and is not a permanent guarantee.
- Check only CodeRadar's configured keychain accounts and documented environment variables. Never hunt for keys in unrelated applications, public repositories, or third-party key lists. Do not publish credentials or credential fragments.
- Metadata/auth checks send no workspace content. A separate bounded synthetic request proves that the configured model and protocol work. Rate or credit metadata alone is not proof of usable inference.
- Support offline baseline, baseline plus optional Jev, and baseline plus optional contextual review with or without Jev. Missing cloud credentials must not block offline correctness improvements or hermetic tests.
- Gemini uses a user-owned Google AI Studio project and key. Free-tier access, model availability, and quotas must be checked for that project; they are not assumed. No billing activation or payment is part of this spec.
- Ollama requires an installed, reachable local runtime and a compatible model. An unreachable default endpoint is not proof that Ollama is uninstalled everywhere. A configured remote endpoint is not treated as local merely because it uses the Ollama protocol.
- Apply explicit cloud-review consent and privacy checks to the entire context bundle and request metadata. Reuse Jev redaction. For Gemini review, refuse bundles in which detected secrets remain or refusal policy is triggered; let the user select offline/local review. Local-only claims require loopback endpoint validation and disabled cross-host redirects.
- Do not retry rejected authentication indefinitely. Use bounded retries for transient failures, respect Retry-After, and preserve completed results. Record fallbacks per work item and cumulative scan provenance; recovery does not erase prior failures.

### 7. Budgets, evidence ingestion, and fixes

- Default scan budget: at most 100 cloud requests across enabled providers, at most 5 minutes of contextual review wall time, and a user-configured cloud cost ceiling defaulting to USD 0.50. These are product caps, not projected charges or latency promises.
- Reserve request and estimated cost budget before dispatch; limit generative output tokens. If pricing or an upper-bound estimate is unavailable, require an explicit request/token budget mode that discloses unavailable cost estimation; do not claim a hard dollar guarantee. Actual provider usage reconciles the ledger after completion.
- Stop scheduling new work when cancelled or a budget expires, interrupt in-flight work where transport permits, and preserve completed work with partial/cancelled status. No new workspace writes follow cancellation.
- A scan-scoped standard-library cancellation signal reaches scanner, sink, scheduler, and provider worker boundaries. Do not promise immediate interruption of blocking HTTP calls: cancellation acknowledgement is immediate, no new work is dispatched, and unavoidable in-flight waits are bounded by configured request timeouts. Do not label the scan terminally cancelled until its worker activity is settled; never detach an untracked worker that could later publish results or write files.
- Cache only results tied to primary and related source hashes, rule set, provider/model version, prompt, threshold policy, review mode, privacy mode, and context policy. Credential changes do not expose cached private data across workspaces or users. Begin with scan-local reuse; persistent sensitive-source caching is deferred.
- Add bounded, read-only ingestion of externally generated static diagnostics through one evidence-import capability, initially SARIF. Treat reports as untrusted input: validate workspace paths and locations, reject stale/unverifiable source identity, deduplicate, preserve tool identity, and cap input size. Do not execute analyzers or project commands inside the product.
- First delivery may rely on native scanner checks; importing results from type/lint/security tools expands evidence only when a user supplies valid diagnostics. Dependencies without lockfile evidence are not described as audited.
- Preserve hash checks, workspace guard, atomic writes, UTF-8 validation, explicit diff review, and the existing only-writer policy for patches. Generated fixes are drafts. Syntax/type/test validation is reported as not run unless trustworthy validation evidence was actually obtained; executing test commands is deferred.
- Add in-process TS/JS syntax validation for both deterministic and generative fix previews. A narrow inner SyntaxValidator port is implemented with the existing infrastructure parser grammars; parsing libraries never move into use cases. Parse the complete proposed content using the correct file dialect before exposing an applicable proposal. Parser error or missing nodes reject application and identify the limitation; if the source was already malformed, a proposal must still produce parseable supported content to pass. Unsupported grammar/parser failure yields unavailable validation and no applicable automated proposal. A successful Chunker call is not validation, because chunking intentionally falls back on parse errors. Syntax success is labeled syntax-only, never type safety, test success, or behavioral correctness. Retain hash/atomic/path guards at apply time.

### 8. Quality evaluation and promotion

- Build at least 100 independently labeled review units: at least 50 known defects and 50 safe lookalikes, covering single-file and cross-file TS/JS cases. Include supported clean patterns, false-positive traps, missing context, staged divergence, and incomplete provider responses. Use licenses/provenance appropriate for fixtures.
- Split by repository or bug family into tuning and held-out sets before threshold/prompt tuning. The held-out set contains at least 25 defects and 25 safe cases; no near-duplicate tuning examples. Record disagreements and adjudication rather than silently rewriting truth to match output.
- Label ownership: the project maintainer or a designated human reviewer approves ground truth from source evidence and, where practical, a reproducing test or documented fix. A second human adjudicates ambiguous cases when available; otherwise exclude disputed cases from promotion and report them separately. A model under evaluation never creates, approves, or adjudicates its own ground truth. AI may assist fixture preparation, but human approval and provenance are mandatory. Lack of approved labels leaves the quality gate open.
- Compare native baseline, baseline plus Jev, baseline plus contextual review, and combined mode. Missing providers produce a not-run comparison, never fabricated metrics.
- Report precision, recall, false positives per reviewed change, location correctness, coverage/completion, provider success/fallback rates, p50/p95 wall time, and actual cost. Publish numerator/denominator and uncertainty; modest sample sizes are a pilot, not universal proof.
- Proposed promotion gate for model-supported findings: held-out precision at least 90%, primary-location correctness at least 95%, and defect recall no more than 5 percentage points below the strongest measured baseline on the same units. Reject an empty-output configuration that only appears precise because it abstains on all defects. Report abstention/coverage and runtime alongside the gate.
- These are initial product acceptance targets, not achieved results. If a configuration misses them, retain its experimental label and tune on the tuning set only. Keep deterministic evidence available independently.

### 9. Ordered delivery

| Slice | Deliverable | Depends on | Acceptance gate |
| --- | --- | --- | --- |
| R0 | Credential/capability baseline and explicit provider states | None | Missing/rejected/unreachable providers cannot appear available; offline scan works |
| R1 | Versioned coverage, nullable score, transactional history migration | None | Unsupported and zero-coverage scans are honest on every surface; old rows survive migration |
| R2 | Immutable staged snapshot and full-scan source identity | R1 | Same index yields same findings despite worktree edits |
| R3 | Complete rule ownership and Jev protocol validation | R0, R1, R2 | Every rule is accounted for; incomplete answers cannot become clean |
| R4 | Evaluation corpus and baseline runner | R1–R3 | Reproducible native baseline with an untouched holdout split |
| R5 | Bounded context and evidence state model | R2–R4 | Related sources/citations are snapshot-correct and budgets are enforced |
| R6 | Optional Gemini/Ollama contextual review and independent changed-code pass | R0, R5 | Hermetic full-scan tests pass; each available provider has a real synthetic and fixture smoke test |
| R7 | External diagnostics import, budgets, cancellation, and reuse | R5, R6 | No silent truncation, stale reuse, escaping diagnostics, or unbounded dispatch |
| R8 | Calibration, comparison, release decision, and documentation | R4–R7 | Measured quality gate plus existing architecture/security/performance/release gates |

Recommended single-developer order: R1, R2, R0, R3, R4, R5, R6, R7, R8. R0 and R1 are logically independent; the numbering does not impose a provider-first prerequisite. Retain one active delivery branch/PR at a time under the repository workflow.

## Testing Decisions

- The user confirmed complete scans and their reports as the primary testing boundary on 2026-11-05. Use injected provider transports/source snapshots for repeatable tests. Keep limited desktop/provider checks separate.
- Prefer behavioral assertions over private helpers, prompt text snapshots, worker counts, or exact internal call sequences. Observe report contents, status transitions, recorded source identities, provider-boundary payloads, and absence of unintended writes.
- Extend the existing scan-pipeline, staged-pipeline, CLI, robustness, history, security, and architecture test patterns. Keep the existing exact 15-finding fixture unless a deliberately accepted rule change requires a documented expectation update. Add a distinct evaluation corpus rather than expanding that fixture into the benchmark.
- Coverage tests exercise Python-only, empty, mixed-language, unreadable, oversized, hidden/excluded, invalid UTF-8, and partial scans. Assert the file partition and score semantics end to end.
- Migration tests open populated legacy databases, preserve every row and finding snapshot, store/reload SQL NULL for new zero-coverage scans, reopen without applying migration twice, and exercise interrupted/failed migration rollback. Assert that a history failure leaves a finished scan usable and visible as a persistence failure.
- Test legacy finding JSON without engine/probability fields and assert unknown/null across history/report DTOs, without rewriting the stored snapshot or inventing Tier 0/1.0. Test valid explicit recorded legacy values independently.
- Git tests use real temporary repositories and assert staged-only bytes, unchanged index under worktree edits/deletion, rename, initial commit, conflicts, and stable source identities.
- Provider contract tests simulate missing keys, keychain errors, unauthorized, forbidden, budget exhaustion, 429 with retry hints, timeouts, malformed/oversized/missing/duplicate answers, unknown rules/types, nonfinite/out-of-range probabilities, mixed fallback and recovery, cancellation, and partial completion.
- Context tests exercise relative imports, aliases, cycles, unresolved/dynamic imports, safeguards in callers, source edits mid-scan, context truncation, injected instructions, secret-bearing related files, escaping paths, and false citations. Assert externally visible evidence outcomes and bounded payloads.
- Exercise a multi-window source whose later code depends on an early import/type; preserve raw chunk offsets while checking the bounded/redacted summary. Test directly changed versus unchanged hunk context, deletions-only changes, related-callsite attribution, and coarse unlocalized matches.
- Fix-preview tests reject malformed proposed TS/TSX/JS/JSX for both template and generative paths, distinguish parser fallback from validation, and accept parseable syntax while leaving type/test status unvalidated. Cancellation tests cover pending blocking requests and assert no late results/writes after terminal cancellation.
- Real-model tests use synthetic/curated non-secret fixtures and run only when access is configured and cloud disclosure is authorized. No live credentials in CI. A skipped live test is an open gate, not a pass.
- Existing patch safety remains mandatory; review and evidence import must not introduce a new workspace writer or process-spawning bypass.
- Run workspace build/test/fmt/clippy, architecture/security guards, MSRV when dependencies or lockfiles change, frontend build, and release memory/performance gates for affected slices. Contextual request budgets are additional caps and do not replace existing local-performance gates. Measure optional review separately and retain raw measurements.

## Out of Scope

- Supporting arbitrary languages or claiming complete security coverage.
- Obtaining other people's API keys, creating paid accounts, activating billing, or bypassing provider access limits.
- Replacing Jev solely because another provider is available, or mandating Jev before every generative review request.
- Automatic model-generated patches, autonomous repository commands, arbitrary tools, or executing tests supplied by scanned projects.
- Full interprocedural data-flow/type analysis built from scratch, IDE extensions, automatic PR comments, team sync, and custom rule builders.
- Editing vendored review-skill documents, broad architectural rewrites, or weakening the dependency/security guards.
- Changing signed rule counts or claiming old human installation/provider gates are complete.

## Further Notes

### Parent tracking

Issue #20 remains the parent specification. These boxes track merged slice PRs, not completion of this planning document. Add child-issue/PR links as execution is commissioned; splitting into nine child issues is a tracking option, not a prerequisite or nine tasks already started.

- [ ] R1 — Coverage, nullable score, and transactional history migration
- [ ] R2 — Captured staged source and source identity
- [ ] R0 — Provider diagnostics and explicit offline mode
- [ ] R3 — Shared detector ownership and Jev protocol completeness
- [ ] R4 — Human-approved corpus, fixed holdout, and baseline
- [ ] R5 — Bounded context and evidence decisions
- [ ] R6 — Optional contextual providers and changed-code pass
- [ ] R7 — Imported diagnostics, budgets, cancellation, and reuse
- [ ] R8 — Calibration, comparison, and promotion decision

The canonical online spec is this issue, so it does not depend on an uncommitted repository-relative link. The first implementation PR must commit the English specification and plan as repository documentation and add permanent repository links here after merge.

### Provider access observed on 2026-11-05

| Capability | Observation | Interpretation |
| --- | --- | --- |
| GitHub Issues | Repository access authenticated; issues enabled | Spec publication available |
| Jev/OpenRouter | CodeRadar keychain contains a credential; environment has none. Key metadata and one tiny synthetic decision each return HTTP 401 | Current stored credential is unusable. Expired vs revoked vs invalid cannot be distinguished from this response |
| Gemini | No credential found in CodeRadar's account or documented environment | Live Gemini testing unavailable in this session |
| Local Ollama | Default loopback model-list endpoint unreachable | Local review is not currently testable at that endpoint; installation elsewhere was not investigated |

The availability check used CodeRadar's existing credential resolver and transport from a temporary helper outside the repository. It sent no workspace source and did not persist or print secrets. The Jev request used only synthetic state. No service account, Google project, API key, billing plan, or model installation was created.

The user can obtain their own Gemini key through [Google AI Studio](https://aistudio.google.com/api-keys). Google's [getting-started guidance](https://aistudio.google.com/docs/get-started) explains project/key setup. [Pricing](https://ai.google.dev/gemini-api/docs/pricing) describes a limited free tier and its data-use differences; eligibility and quotas must be checked in the user's project. [Available regions](https://ai.google.dev/gemini-api/docs/available-regions) lists Egypt, but regional support does not guarantee account eligibility. A fresh authorized Jev/OpenRouter key must replace the rejected one before live Jev work can pass its gate.

API facts: [Jev's decision-model role](https://docs.typesafe.ai/introduction/coding-agents), [Noul uncertainty](https://docs.typesafe.ai/confidence), [OpenRouter Jev protocol](https://openrouter.ai/docs/guides/community/jev), [key/limit checks](https://openrouter.ai/docs/api-reference/limits), and [Ollama model discovery](https://docs.ollama.com/api/tags).

The prior workspace tests and frontend build passed during the assessment. They were not rerun as part of this documentation-only specification task. No implementation or real-model quality improvement is claimed by publishing this specification.
