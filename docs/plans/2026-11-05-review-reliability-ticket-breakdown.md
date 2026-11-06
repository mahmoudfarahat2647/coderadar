# Review Reliability — Published Phases and Tickets

Date: 2026-11-05. Status: approved by the user and published as seven GitHub issues under three milestones. Parent #20 body, state, labels, milestone, and comments were verified unchanged.

Source: [CR-SPEC-17](https://github.com/mahmoudfarahat2647/coderadar/issues/20) and [implementation plan](https://github.com/mahmoudfarahat2647/coderadar/issues/20#issuecomment-6667638276). This replaces the 15-ticket proposal with three approved phases and seven published delivery tickets. The core correctness/evidence/privacy/quality gates remain mandatory.

## Phase 1 — Trustworthy offline review

Tickets P1-A and P1-B can be implemented in parallel after agreeing additive boundary contracts. P1-A owns coverage/history/source identity; P1-B owns engine/provider behavior. Each branch remains independently testable against its starting public contracts. Merge one green PR at a time; the second rebases and reconciles shared report/history/status fields before the phase gate. Parallel implementation does not permit skipping integration checks or weakening native blockers.

1. [P1-A — Make scan reports and source identity trustworthy](https://github.com/mahmoudfarahat2647/coderadar/issues/21). Blocked by: none. Delivers: Complete scans report honest coverage and unavailable scores, preserve legacy history safely, and analyze captured staged blobs rather than changing working-tree files.
2. [P1-B — Make rule execution and provider status honest](https://github.com/mahmoudfarahat2647/coderadar/issues/22). Blocked by: none. Delivers: All enabled rules run under deliberate ownership, invalid Jev answers cannot become clean findings, and users can distinguish unavailable providers and mixed fallback scans from fully live execution.

Phase gate: honest scores/history, source-correct staged findings, complete rule ownership, rejected protocol failures, and explicit provider/degradation states all work together in complete offline/fake-provider scans. Working paid credentials are not required.

## Phase 2 — Evidence and evaluation readiness

These three tickets start after Phase 1 and can proceed in parallel. Freeze a small vendor-neutral review-work contract first: source/evidence/context versions and budgets are explicit plain input data. Context retrieval does not implement a second scheduler; the scheduler does not implement a second context resolver. Native corpus labeling/baseline does not wait for either. Resolve shared-contract edits before merging, then run the integrated gate.

3. [P2-A — Review bounded repository context and ground PR findings](https://github.com/mahmoudfarahat2647/coderadar/issues/23). Blocked by: P1-A, P1-B. Delivers: Candidates receive snapshot-correct related context, precise evidence and honest verdicts, and PR results distinguish changed/related code from outside-range debt.
4. [P2-B — Establish the evaluation corpus and native baseline](https://github.com/mahmoudfarahat2647/coderadar/issues/24). Blocked by: P1-A, P1-B. Delivers: A maintainer has independently approved defects and safe lookalikes, a frozen holdout, and reproducible native results before model selection or threshold tuning.
5. [P2-C — Control review budgets and cancellation](https://github.com/mahmoudfarahat2647/coderadar/issues/25). Blocked by: P1-A, P1-B. Delivers: Users can bound and cancel cloud work without losing finished results or allowing hidden requests and late writes.

Phase gate: fake-reviewer complete scans produce bounded, grounded, scope-attributed results; cancellation and budgets work; the human-approved corpus/holdout and native baseline are reproducible. Missing approved labels are reported as a genuine open gate, not waived for speed.

## Phase 3 — Live integration and measured release decision

6. [P3-A — Integrate optional AI review and syntax-safe fix previews](https://github.com/mahmoudfarahat2647/coderadar/issues/26). Blocked by: P2-A, P2-C. Delivers: A developer can opt into local Ollama or user-owned Gemini contextual review, and both template/generated fix previews reject syntactically invalid TS/JS before application.
7. [P3-B — Measure effectiveness and make the release decision](https://github.com/mahmoudfarahat2647/coderadar/issues/27). Blocked by: P2-B, P3-A. Delivers: A maintainer receives reproducible accuracy/cost measurements and an explicit promote-or-remain-experimental decision for each actually tested configuration.

The evaluation harness/reporting preparation can overlap adapter work using the already approved corpus. Final quality measurement and closing P3-B must wait for P3-A: there is no honest way to measure the finished integration before it exists. A whole-phase human completion gate waits for both tickets even though coding preparation may overlap.

Phase gate: available provider/model configurations have real synthetic/curated smoke evidence plus the specified quality measurements and release checks. Missing provider access remains open/experimental; fake-provider tests never substitute for measured effectiveness. An offline reliability release can be evaluated separately.

## Approved scope deferral for a faster first release

- Move external SARIF import to a later enhancement, since it does not repair the current false-clean, staged-source, missing-rule or grounding defects.
- Move enhanced scan-local caching to later measurement-driven optimization; do not introduce a weaker persistent or chunk-only cache in its place.
- Persistent sensitive-source caching was already out of scope and remains deferred.
- Keep syntax-only fix validation, all core correctness fixes, context/evidence, the independent changed-code pass, cloud privacy, budgets/cancellation, the full human-approved holdout, and quality promotion gates in the first release.

The user approved these first-release deferrals on 2026-11-05. They are explicitly recorded in all seven issue bodies. The parent specification remains preserved as required by to-tickets; no deferred work is represented as completed.

## Mapping from the superseded proposal

| New ticket | Previous coverage |
| --- | --- |
| P1-A | T01 + T04 |
| P1-B | T02 + T03 + T05 |
| P2-A | T07 + T08 |
| P2-B | T06 |
| P2-C | T09 |
| P3-A | T10 + T11 + T12 |
| P3-B | T15, with approved first-release deferrals explicitly recorded |
| Later enhancements | T13 (SARIF) + T14 (enhanced caching) |

Merging tickets reduces tracking overhead and coordination; parallelizing independently testable work reduces elapsed time. Neither eliminates migration, labeling, validation or source-analysis work. The combined tickets are sized as complete phase deliverables; implementers can use internal red/green steps without creating another GitHub issue for each step.

## Approval and publication

The user approved the revised breakdown and stated scope deferrals before publication. All seven issues carry ready-for-agent; native GitHub blocked-by relationships match the approved graph. No application implementation has started.

## GitHub tracking

| Phase | Milestone | Issues |
| --- | --- | --- |
| 1 | [Review Reliability P1 - Trustworthy scanning](https://github.com/mahmoudfarahat2647/coderadar/milestone/10) | [#21](https://github.com/mahmoudfarahat2647/coderadar/issues/21), [#22](https://github.com/mahmoudfarahat2647/coderadar/issues/22) |
| 2 | [Review Reliability P2 - Evidence and evaluation readiness](https://github.com/mahmoudfarahat2647/coderadar/milestone/11) | [#23](https://github.com/mahmoudfarahat2647/coderadar/issues/23), [#24](https://github.com/mahmoudfarahat2647/coderadar/issues/24), [#25](https://github.com/mahmoudfarahat2647/coderadar/issues/25) |
| 3 | [Review Reliability P3 - AI integration and measured release](https://github.com/mahmoudfarahat2647/coderadar/milestone/12) | [#26](https://github.com/mahmoudfarahat2647/coderadar/issues/26), [#27](https://github.com/mahmoudfarahat2647/coderadar/issues/27) |

Current frontier: #21 and #22. Phase 2 has three independent tickets after Phase 1. Final quality measurement in #27 waits for integration #26, although evaluation preparation can overlap.
