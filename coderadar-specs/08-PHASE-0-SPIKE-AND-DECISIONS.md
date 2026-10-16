# 📡 CodeRadar Studio: Phase 0 — Jev Spike Results & Phase-0 Decisions

> **Document ID:** `CR-SPEC-08`
> **Status:** Complete (one open blocker: authenticated Jev benchmark — see §6)
> **Milestone:** [Phase 0 — Spike & spec cleanup](https://github.com/mahmoudfarahat2647/coderadar/milestone/1)
> **Closes task line:** Issue [#1](https://github.com/mahmoudfarahat2647/coderadar/issues/1)
> **Executed:** 2026-10-16
> **Gate:** *"A written benchmark number. If Jev is significantly slower than 35 ms, Mock-first is still fine, but update the PRD claims."* → **Gate result: PASS (with a documented SLA correction).**

---

## 1. TL;DR — the written benchmark

| Metric | Value |
| :--- | :--- |
| **Jev live per-request latency (vendor-published)** | **70–500 ms** per `/v1/decide` call (all questions evaluated in parallel, one round trip) |
| **Amortized per-chunk (batch of 10, warm HTTP/2)** | **≈ 16–59 ms/chunk** = (70–500 ms model + ~90 ms RTT) ÷ 10 |
| **Cold first request from this network** | TCP connect ~95 ms + TLS ~262 ms → added ~250 ms before the model runs |
| **Amortized per-chunk (cold connection)** | ≈ 25–75 ms/chunk |
| **Spec's original claim** | 12–35 ms/chunk amortized |
| **Verdict** | The **12 ms floor is not reachable** and the **35 ms ceiling is only reachable at the optimistic end of Jev's own range**. The spec claim is **not validated as written**. |

**Decision:** keep Mock-first as the default engine (already the plan), and **restate the live Tier-1 SLA as `≤ 500 ms per batched request` / `≈ 16–59 ms per chunk amortized`** instead of "12–35 ms per chunk". Mock mode keeps its deterministic synthetic latency (10–35 ms/chunk). See §3.

---

## 2. Spike methodology & raw measurements

### 2.1 Endpoints probed

Jev is a real hosted, typed-decision API (TypeSafe AI "System One" model). Two hosts, same request shape:

- **Hosted gateway:** `POST https://jevtypesafeai.com/api/v1/decide` (`Authorization: Bearer jv_live_…`)
- **Official TypeSafe:** `POST https://api.typesafe.ai/v1/systemone` (`Authorization: Bearer <key>`)

A decision call sends `state` + a map of typed `questions` (`choice` / `score` / `noul`) and returns typed `answers` plus calibrated confidences. There is **no text generation** — this part of the spec is accurate.

### 2.2 What was measured from the real network

No Jev API key was available in this environment (no `JEV_API_KEY` in the environment), so the inference step could not be exercised. The **transport** was measured directly from this workstation against both live hosts. Requests were intentionally unauthenticated, so the auth layer rejects them before inference — the recorded time is a **pure network/TLS floor**, not model latency.

Command shape (PowerShell 5.1, `curl 8.13.0 / Schannel` — HTTP/1.1 only, this libcurl build has no HTTP/2):

```
curl -sS -o NUL -w "dns=%{time_namelookup} connect=%{time_connect} tls=%{time_appconnect} ttfb=%{time_starttransfer} total=%{time_total} http=%{http_code}\n" \
  -X POST "https://jevtypesafeai.com/api/v1/decide" \
  -H "Content-Type: application/json" \
  --data-binary "@jev-body.json"
```

Probe body: one `state` + two questions (`noul` injection check, `score` severity) — ≈ 250 bytes.

### 2.3 Raw results — `jevtypesafeai.com/api/v1/decide`

| Run | DNS (s) | TCP connect (s) | TLS (s) | TTFB (s) | Total (s) | HTTP | Bytes | Proto |
| :--- | ---: | ---: | ---: | ---: | ---: | :---: | ---: | :---: |
| 1 | 0.0059 | 0.0939 | 0.2682 | 0.4755 | 0.4760 | 401 | 70 | 1.1 |
| 2 | 0.0066 | 0.1053 | 0.2816 | 0.4823 | 0.4835 | 401 | 70 | 1.1 |
| 3 | 0.0069 | 0.0847 | 0.2511 | 0.4976 | 0.4980 | 401 | 70 | 1.1 |
| 4 | 0.0057 | 0.0936 | 0.2539 | 0.4418 | 0.4505 | 401 | 70 | 1.1 |
| 5 | 0.0064 | 0.0984 | 0.2571 | 0.4631 | 0.4644 | 401 | 70 | 1.1 |
| **Mean** | **0.0063** | **0.0952** | **0.2624** | **0.4721** | **0.4745** | | | |

Response body: `{"error":"Missing API key. Send 'Authorization: Bearer jv_live_...'."}`

### 2.4 Raw results — `api.typesafe.ai/v1/systemone`

| DNS (s) | TCP connect (s) | TLS (s) | TTFB (s) | Total (s) | HTTP | Bytes | Proto |
| ---: | ---: | ---: | ---: | ---: | :---: | ---: | :---: |
| 0.0072 | 0.1015 | 0.2711 | 0.5307 | 0.5310 | 403 | 118 | 1.1 |

Response body: `{"detail":{"error_type":"authentication_error","message":"Must supply an API key! Check your request and try again."}}`

**Transport reading:** ~6 ms DNS, ~95 ms TCP RTT, ~262 ms TLS handshake, ~470–530 ms to first byte. Both hosts are live and reachable from this network; **network RTT is ~90–105 ms**, which by itself is already ~3× the spec's *entire* 35 ms budget.

### 2.5 Vendor-published figures (used where measurement was blocked)

From the Jev API docs / pricing pages (retrieved 2026-10-16):

- **Response latency: 70–500 ms** per call (multiple questions answered in parallel).
- **Pricing:** **$0.42 / 1M input tokens**, output free, ≈ **$0.001 per decision**.
- **Limits:** 250,000 tokens/s, **1,200 requests/min**; **64k tokens** combined `state` + all questions (32k for `state` + single longest question), raisable to ~262k via a preview header.
- Model pinning is recommended in production (`jev-1.13.0`), because `jev-latest` thresholds can drift.

### 2.6 Derived numbers for CodeRadar's pipeline

Assumptions: chunk ≤ **12 KB** (per `CR-SPEC-03`) ≈ ~3,000 tokens; batch of **10 chunks** per request; warm HTTP/2 connection.

| Scenario | Per-request | Per-chunk amortized |
| :--- | :--- | :--- |
| Vendor floor (70 ms model + ~90 ms RTT) | ~160 ms | **~16 ms/chunk** |
| Vendor mid (285 ms model + ~90 ms RTT) | ~375 ms | **~38 ms/chunk** |
| Vendor ceiling (500 ms model + ~90 ms RTT) | ~590 ms | **~59 ms/chunk** |
| Cold connection (adds ~250 ms TLS) | 410–840 ms | ~41–84 ms/chunk |

**Cost sanity check:** a 12 KB chunk ≈ ~3,000 input tokens ≈ **$0.00126/chunk**. A 5,000-file project at ~1.5 chunks/file ≈ ~$9.4 per full scan under live Jev — near-zero versus LLM review, but not literally "$0.0001/file" as `CR-SPEC-00` implies.

**Batching caveat (architectural risk for Phase 5):** Jev evaluates **one `state`** with a map of questions; it does not natively fan out *N independent code chunks* as *N independent decisions* in one call. The spec's "batches of 10 chunks per HTTP/2 request" therefore needs a concrete mapping (e.g. `state` = array of chunks + one repeated per-element question, or 10 separate requests on one kept-alive connection). Either way the parallelism benefit is smaller than the spec assumes. Flagged here; to be resolved in Phase 5.

---

## 3. Decision — Latency SLA reconciliation (`CR-SPEC-01` FR-04)

The docs shipped with two figures: **12–30 ms** (`CR-SPEC-00` §1/§2) and **12–35 ms** (`CR-SPEC-01` FR-04, `CR-SPEC-03`, `CR-SPEC-06`). The spike shows **neither number describes live Jev**. Resolution:

> **Single source of truth — the two-tier latency model:**
>
> - **Mock / Tier 0 (default, offline-capable):** synthetic **10–35 ms per chunk** (deterministic heuristics; already specified in `CR-SPEC-03` §4).
> - **Live Jev Tier 1:** **≤ 500 ms per batched decision request** (p95), **≈ 16–59 ms per chunk amortized** over a batch of 10 on a warm HTTP/2 connection. Vendor model floor is **70 ms/request**; do not publish a sub-70 ms per-request promise.
>
> **Do not claim "12 ms" for live Jev** — it is below the vendor's own floor and below this network's RTT. The aspirational `12–35 ms` figure survives only as the Mock-mode synthetic band.

Documentation was updated accordingly:

- `CR-SPEC-00` §1 and §2: **12–30 ms → 12–35 ms** (harmonized) and re-labelled as the **Mock-mode synthetic band**, with a pointer to this document for live Jev.
- `CR-SPEC-01` FR-04: kept `12ms–35ms` but scoped to **Mock mode**, and added the **live Jev budget (≤ 500 ms/request, 16–59 ms/chunk amortized)**.
- `CR-SPEC-03`: the headline latency/cost block and §3 were corrected to **70–500 ms/request (~16–59 ms/chunk amortized)** and **~$0.001/chunk**; the `12–35 ms` band is now explicitly Mock-only.
- `CR-SPEC-06`: the review question was rewritten to ask about the two-tier model instead of assuming 12–35 ms.

**Consequence for the PRD/positioning:** Mock-first remains the correct default (per the Phase 0 gate). The marketing claim "12–30 ms per file using Jev" must be softened — see the note added to `CR-SPEC-00`.

---

## 4. Decision — Tier 1 languages for v1.0

**Decision: TypeScript / JavaScript first** (as suggested by `PHASES.md`).

- Tier 0 syntactic rules (`ARCH-DEP-001`, `SEC-TOK-002`, `SEC-TLS-005`) and the tree-sitter chunker ship for **TS/JS only** in v1.0.
- Rationale: matches the `d:/review` corpus (the source skills are TS/ECMAScript-heavy), the React 19 frontend, and the Stitch UI examples (all `.ts`/`.py`/`.go` snippets are illustrative, but TS/JS is the common denominator).
- **Explicitly deferred to v1.5+:** Python, Go, Rust, Java. `PHASES.md` Phase 2 ("tree-sitter for TS/JS") and Phase 5 already assume this — no doc change needed beyond recording the decision here.

---

## 5. Doc consistency validation

### 5.1 Radar node omission invariant — ✅ CONSISTENT (no change needed)

The invariant is stated identically in `CR-SPEC-01` FR-02 and `CR-SPEC-05` §B: **only active findings (`LOW`/`MEDIUM`/`HIGH`/`CRITICAL`, i.e. `s > 0`) draw canvas nodes; clean files draw nothing** and are represented via HUD telemetry + ambient glow. `Severity::Clean` exists in the domain (`CR-SPEC-02`) but produces no node. Validated — no edit required.

### 5.2 Radar node capacity — ⚠️ INCONSISTENT → fixed

Three different capacities were implied:

- `CR-SPEC-05` §D: **clustering above 150 nodes**.
- `PHASES.md` Phase 4 gate / `CR-SPEC-07`: **60 FPS with 100 nodes**.
- `CR-SPEC-06` review prompt: **50–100 animated nodes**.

Fixed by stating one capacity: **60 FPS target up to 150 nodes; clustering activates above 150** (`CR-SPEC-05` §D, `CR-SPEC-06` §3, `PHASES.md`/`CR-SPEC-07` Phase 4).

### 5.3 Markdown export priority — ⚠️ INCONSISTENT → fixed

`CR-SPEC-01` §5 scope matrix lists **F8 "Export Report as Markdown" as in-scope for MVP (v1.0)**, and `CR-SPEC-07` Phase 7 schedules it. However, the **functional-requirements list (FR-01…FR-06) had no export requirement** — the scope matrix promised a feature with no FR. Fixed by adding **FR-07: Markdown Report Export** to `CR-SPEC-01`. Priority confirmed: **P1 within MVP**, delivered in Phase 7.

### 5.4 Broken companion link — ⚠️ → fixed

Both `PHASES.md` and `CR-SPEC-07` opened with `Companion to [PRD.md](./PRD.md)`, but no `PRD.md` exists. Repointed to [`coderadar-specs/01-BRD-AND-MVP-REQUIREMENTS.md`](./01-BRD-AND-MVP-REQUIREMENTS.md) (the actual BRD/PRD).

### 5.5 Milestone task drift — ⚠️ → fixed

Phase 0 said *"Create GitHub milestones `Phase 1` … `Phase 7`"*, but the milestones were already created and actually span **`Phase 0` … `Phase 8`**. Line corrected to *verify* the existing set (see §7).

---

## 6. Open blocker (does not block the gate)

**Authenticated live-Jev benchmark.** No `JEV_API_KEY` was available, so real decision latency could not be measured end-to-end — only transport + vendor figures. To close this completely:

1. Obtain a hosted `jv_live_…` key (self-serve, prepaid, ≈ $0.03 is enough for dozens of calls).
2. Run 50–100 calls with a 12 KB representative chunk over a **kept-alive HTTP/2** connection (`reqwest` with `http2_prior_knowledge`); record p50/p95 and payload bytes.
3. Re-confirm the §3 SLA in Phase 5 (`PHASES.md` Phase 5: *"Benchmark vs the Phase 0 numbers"*).

This is tracked as the one remaining sub-item of the Phase 0 task and is scheduled to be satisfied by the Phase 5 benchmark against these numbers.

---

## 7. Milestones verification

`gh api repos/mahmoudfarahat2647/coderadar/milestones` confirms **9 milestones already exist**, matching `PHASES.md` Phases 0–8 one-to-one:

| # | Milestone | State | Open issues |
| :---: | :--- | :---: | :---: |
| 1 | Phase 0 — Spike & spec cleanup | open | 1 |
| 2 | Phase 1 — Foundation | open | 1 |
| 3 | Phase 2 — Scanner, Tier 0, Mock engine | open | 1 |
| 4 | Phase 3 — Tauri shell, dashboard, findings feed | open | 1 |
| 5 | Phase 4 — Polar radar canvas | open | 1 |
| 6 | Phase 5 — Jev Tier 1 + privacy layer | open | 1 |
| 7 | Phase 6 — Diff modal, safe patching, history | open | 1 |
| 8 | Phase 7 — Hardening and release v1.0 | open | 1 |
| 9 | Phase 8 — Post-MVP (v1.1+) | open | 1 |

**Result:** ✅ verified — no milestone creation required. ✅

---

## 8. Phase 0 task ledger

| Task (from issue #1) | Status | Evidence |
| :--- | :---: | :--- |
| Call the Jev API with 10-chunk batches over HTTP/2; record latency & payload | ⚠️ partial | Endpoints probed live (§2.3–2.4); authenticated inference blocked on key (§6) |
| Reconcile latency SLA vs `CR-SPEC-01` EF-04 target | ✅ | §3 — two-tier model adopted |
| Choose Tier 1 languages for v1.0 | ✅ | §4 — TypeScript/JavaScript |
| Validate radar node-omission invariant | ✅ | §5.1 — consistent |
| Fix radar node limit + Markdown export priority inconsistencies | ✅ | §5.2, §5.3 |
| Fix broken `PRD.md` companion link | ✅ | §5.4 |
| Verify GitHub milestones Phase 0 … Phase 8 | ✅ | §7 |

**Gate:** ✅ **PASS** — a written benchmark number exists (vendor 70–500 ms/request; measured RTT ~90 ms, TLS ~262 ms), and the PRD claims have been corrected to match it.
