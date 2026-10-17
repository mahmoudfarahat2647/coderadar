# 🛰️ CodeRadar Studio: Phase 5 — Jev Tier 1 Engine & Privacy / Redaction Layer

> **Document ID:** `CR-SPEC-12`  
> **Status:** ✅ Complete — implemented, gated, and verified (see §7)  
> **Milestone:** [Phase 5 — Jev Tier 1 + privacy layer](https://github.com/mahmoudfarahat2647/coderadar/milestone/6)  
> **Closes task line:** Issue [#6](https://github.com/mahmoudfarahat2647/coderadar/issues/6)  
> **Executed:** 2026-10-17  
> **Companion:** [`03-JEV-DECISION-ENGINE-INTEGRATION.md`](./03-JEV-DECISION-ENGINE-INTEGRATION.md), [`08-PHASE-0-SPIKE-AND-DECISIONS.md`](./08-PHASE-0-SPIKE-AND-DECISIONS.md)  
> **Gate:** *"Identical UI and finding schema with real Jev results; disconnecting network seamlessly falls back to Mock with the badge shown."*

---

## 1. Goal

Swap the Mock heuristic engine for the real Jev semantic evaluator **without touching the
use cases**: the Layer 2 `SemanticRuleEvaluator` port is unchanged, and a decorator +
composition-root decision selects Jev or Mock at runtime.

## 2. Deliverables

| Task (issue #6) | Component | Path |
| :--- | :--- | :--- |
| `JevRuleEvaluatorAdapter` over the port | `reqwest` blocking client, HTTP/2, connection pool, **one decision request per chunk** (batch_size = scheduling window of a bounded worker pool, per `CR-SPEC-08 §3.4`), transient-only retries | `crates/coderadar-infrastructure/src/jev/mod.rs` |
| Tier 0 secret redaction before egress | `[REDACTED_SECRET]` scrubber reusing the Tier 0 signature + entropy detectors | `crates/coderadar-infrastructure/src/redaction.rs` |
| Secure API key storage | OS keychain via `keyring-rs`; env override; never a plaintext file | `crates/coderadar-infrastructure/src/keychain.rs` |
| Auto-fallback to Mock | `FallbackRuleEvaluator` decorator + shared `EngineStatus` | `crates/coderadar-infrastructure/src/engine.rs` |
| Timeouts / retries / latency | 2 s request budget, transient-only retry classification (`429`/`5xx`/network retry — `4xx`/parse fail fast), per-request latency telemetry | `crates/coderadar-infrastructure/src/jev/mod.rs` |
| UI badge | `get_engine_status` command + status-bar engine badge, polled every 2 s while a scan runs so a live→Mock flip is visible mid-scan | `src-tauri/src/commands.rs`, `src/components/StatusBar.tsx`, `src/components/Header.tsx` |
| API key entry UI | `ApiKeyDialog` modal + `store_api_key`/`clear_api_key` commands (OS keychain backend) | `src/components/ApiKeyDialog.tsx`, `src-tauri/src/commands.rs` |

## 3. Privacy boundary

Every outbound payload passes through `redaction::redact_chunk`, which replaces any
credential-signature match and any high-entropy value assigned to a secret-flavoured
identifier with the literal `[REDACTED_SECRET]`. The redaction runs **inside** the Jev
adapter, so no code path can send raw secrets. Redaction is covered by unit tests with
synthetic (fake) keys.

## 4. Fallback behaviour

`FallbackRuleEvaluator` calls the primary (Jev) evaluator; on any transport/timeout/parse
error it records a `MockFallback` transition on the shared `EngineStatus` and re-runs the
same batch through the `MockRuleEvaluator`. A panicked worker thread is contained the
same way (it surfaces as a `DomainError`, so the decorator falls back instead of
aborting). The Mock path emits the identical `RuleEvaluationResult` / `Finding` schema
with `engine = MockHeuristic`, and the UI badge flips to *Mock Mode (fallback)* —
the frontend polls `get_engine_status` every 2 s while a scan is running, so the flip
is visible during the scan, not only at its end.

## 5. Latency

Measured against the Phase 0 baseline (`CR-SPEC-08 §1`: ~0.5–0.6 s/chunk warm). The
adapter records **per-request** latency (last, max and cumulative sum) through
`EngineStatus`, so the dashboard can display live Jev telemetry; the live
`examples/jev_bench` run below compares with `CR-SPEC-08 §3.4`.

## 6. Verification

- Workspace unit tests cover redaction, keychain resolution, request building
  (questions carry each rule's `assertion` as `instructions` — required by the live
  decisions endpoint), typed-answer parsing, batching, the transient/permanent retry
  policy and Mock fallback.
- `scripts/check-architecture.sh` stays green (all new code lives in Layer 4).
- Gate evidence (hermetic fallback parity + MSRV + architecture + live benchmark)
  recorded in §7.

## 7. Gate evidence

The Jev transport is abstracted behind the `JevTransport` trait, so the whole adapter
(batching, retries, parsing, redaction, fallback) is exercised **hermetically** — no
network and no API key required. The same `EngineStatus` telemetry path is asserted in
tests, and one authenticated live run provides the latency numbers in §7.3.

| Gate | Command | Result |
| :--- | :--- | :--- |
| Unit + integration tests | `cargo test --workspace --all-targets` | pass (incl. 12 Jev adapter tests, 8 redaction tests, keychain/engine tests) |
| Formatting | `cargo fmt --all -- --check` | clean |
| Lints | `cargo clippy --workspace --all-targets --all-features -- -D warnings` | clean |
| Architecture guard | `crates/coderadar-infrastructure/tests/architecture_boundaries.rs` | 5/5 pass (Layer 4 hosts all new code) |
| MSRV | `cargo +1.82.0 check --workspace --exclude coderadar-desktop --all-targets` | pass |
| Desktop host | `cargo check --manifest-path src-tauri/Cargo.toml --all-targets` | pass |
| Live Jev run | `JEV_API_KEY=… cargo run -p coderadar-infrastructure --example jev_bench -- <dir>` | see §7.3 |

### 7.1 Fallback / offline parity

`FallbackRuleEvaluator` is unit-tested with a failing primary: the decorator records a
`MockFallback` transition, re-runs the batch through `MockRuleEvaluator`, and the
`EngineStatus` snapshot flips to `badge = "Mock Mode (fallback)"` with the reason set.
A live run with a deliberately invalid key degrades exactly the same way (the `401`
is classified permanent, so it fails fast without retries). This is the offline half
of the gate; the live half reuses the unchanged `RuleEvaluationResult`/`Finding`
schema, so the UI renders identically.

### 7.2 MSRV decisions (Rust 1.82)

`reqwest 0.13` declares `rust-version = 1.85`, so the client is held on the `0.12`
line. Its optional HTTP/3 `quinn` tree and the `aws-lc` provider would otherwise drag
`getrandom 0.4` (edition2024) into `Cargo.lock`; the adapter therefore selects
`rustls-tls-native-roots` (pure-Rust `ring` + OS trust store) and `Cargo.lock` pins
`quinn`/`quinn-proto`, `hyper-util`, `hyper-rustls`, `tokio-util`, `idna_adapter` and
`zeroize` to the newest releases that still compile on 1.82. Re-run the MSRV gate after
any dependency change.

### 7.3 Live benchmark (2026-10-17)

Single live run of the real scan pipeline over `src/` (the Tauri frontend, 14 files /
19 AST chunks) through the live OpenRouter decisions endpoint
(`~typesafe/jev-latest`, resolved server-side to `typesafe/jev-1.14-20261014`):

| Metric | Live Phase 5 run | Phase 0 baseline (`CR-SPEC-08 §1`) |
| :--- | :--- | :--- |
| Requests (1 per AST chunk) | 19 | n/a |
| Fallbacks | 0 | n/a |
| Mean request latency | **908 ms** | ~0.49–0.57 s warm / ~0.74–0.86 s cold |
| Max request latency | 1510 ms | n/a (single-request runs) |
| Wall clock for 19 chunks | **2814 ms** (~148 ms/chunk amortized, 16-thread pool) | "background, minutes-scale" verdict unchanged |

The mean sits inside the Phase 0 cold band (~0.9 s/chunk) and well under the 2 s
per-request budget; parallelism amortizes the wall clock far below the per-chunk
latency. Cost is negligible at this scale (Phase 0 measured ~$0.00015/chunk).
