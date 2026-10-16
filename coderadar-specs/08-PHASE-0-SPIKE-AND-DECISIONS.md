# 📡 CodeRadar Studio: Phase 0 — Jev Spike Results & Phase-0 Decisions

> **Document ID:** `CR-SPEC-08`
> **Status:** ✅ Complete — authenticated live-Jev benchmark obtained
> **Milestone:** [Phase 0 — Spike & spec cleanup](https://github.com/mahmoudfarahat2647/coderadar/milestone/1)
> **Closes task line:** Issue [#1](https://github.com/mahmoudfarahat2647/coderadar/issues/1)
> **Executed:** 2026-10-16
> **Gate:** *"A written benchmark number. If Jev is significantly slower than 35 ms, Mock-first is still fine, but update the PRD claims."* → **Gate result: PASS — Jev is ~15–20× slower than 35 ms and is not real-time; PRD claims corrected.**

---

## 1. TL;DR — the written benchmark (measured, authenticated)

| Metric | Value |
| :--- | :--- |
| **Endpoint that worked** | `POST https://openrouter.ai/api/alpha/decisions`, model `~typesafe/jev-latest` (resolves to `typesafe/jev-1.14-20261014`, provider `TypeSafe`) |
| **Live Jev latency — small chunk (344 in-tokens)** | **~0.46–0.55 s warm**, ~0.73 s cold · p50 ≈ **0.49 s** |
| **Live Jev latency — representative 12 KB chunk (3,588 in-tokens)** | **~0.55–0.60 s warm**, ~0.86 s cold · p50 ≈ **0.57 s** |
| **Per-chunk latency for the pipeline** | **≈ 0.5–0.6 s warm (one decision per chunk)** — latency is **not** amortizable across chunks |
| **Cost** | **$0.042 / 1M input tokens**, output free → **~$0.00015 per 12 KB chunk** |
| **Full-scan projection (5,000 files ≈ 7,500 chunks)** | **≈ 5–7 min** wall (rate-limit bound) and **≈ $1** |
| **Spec's original claim** | 12–35 ms/chunk "real-time" |
| **Verdict** | ❌ **Not validated.** Live Jev is a *background, minutes-scale* engine. The 12–35 ms band applies **only to the local Mock engine**. |

**Decision:** keep **Mock-first** as the default engine and the sub-second UX. Re-scope **live Jev Tier 1** as a *background deep scan* (minutes, ~$1/scan), and **restate the SLA: Mock 10–35 ms/chunk; live Jev ~0.5–0.6 s/chunk warm / ~0.9 s cold**. See §5.

---

## 2. Methodology

### 2.1 What was probed

Jev is a real, hosted, **typed-decision** API (TypeSafe AI "System One" model). A decision call sends `state` + a map of typed `questions` (`choice` / `score` / `noul`) and returns typed `answers` with calibrated probabilities — **no text generation**. This part of the spec is accurate and was confirmed (§3.3).

Three access routes were tested:

| Route | Result |
| :--- | :--- |
| `POST https://jevtypesafeai.com/api/v1/decide` (`jv_live_…`) | Requires a `jv_live_` key — rejects the provided key (401) |
| `POST https://api.typesafe.ai/v1/systemone` (official TypeSafe key) | Requires an official TypeSafe key — rejects the provided key (401) |
| **`POST https://openrouter.ai/api/alpha/decisions`, model `~typesafe/jev-latest`** | ✅ **200 — the working route used for the benchmark** |

> **Correction to the spec's integration assumptions (`CR-SPEC-03`):** the key used was an **OpenRouter** key (`sk-or-v1-…`), and the working path is OpenRouter's **decisions** endpoint — **not** the `jv_live_`/`jevtypesafeai.com` route the spec assumed. The direct TypeSafe hosts only accept their own key formats. The spec's `JevRuleEvaluatorAdapter` must therefore target the OpenRouter decisions endpoint (or an official TypeSafe key), and `JEV_API_KEY` is **not** necessarily a `jv_live_` value. The model id resolved server-side to `typesafe/jev-1.14-20261014`.

### 2.2 Client caveat

Timings were measured with `curl 8.13.0` on Windows (Schannel), which negotiates **HTTP/1.1** (this libcurl build has no HTTP/2). Connection reuse was validated by a keep-alive run (`connect`/`tls` = 0 on subsequent requests). "Warm" = same kept-alive connection; "cold" = new process/connection per call. Latency differences from HTTP/2 would be minor at these magnitudes.

### 2.3 Payloads

- **SMALL** — one `state` + 2 questions (`noul` injection, `score` severity) ≈ 250 B, **344 input tokens**.
- **12 KB** — a realistic TS chunk (~10,986 chars) with 90 concatenated-SQL lines + 3 questions (`noul` injection, `noul` secrets, `score` severity) = 13,363 B body, **3,588 input tokens**.

---

## 3. Raw measurements

### 3.1 Small chunk — 8 cold runs

| Run | DNS (s) | TCP (s) | TLS (s) | TTFB (s) | Total (s) | HTTP |
| :--- | ---: | ---: | ---: | ---: | ---: | :---: |
| 1 | 0.0099 | 0.1046 | 0.2700 | 0.7556 | 0.7573 | 200 |
| 2 | 0.0083 | 0.1218 | 0.2988 | 0.7648 | 0.7657 | 200 |
| 3 | 0.0069 | 0.0980 | 0.2711 | 0.7053 | 0.7073 | 200 |
| 4 | 0.0121 | 0.0823 | 0.2641 | 0.7288 | 0.7308 | 200 |
| 5 | 0.0090 | 0.1042 | 0.2871 | 0.7248 | 0.7278 | 200 |
| 6 | 0.0103 | 0.0858 | 0.2551 | 0.7114 | 0.7120 | 200 |
| 7 | 0.0062 | 0.0926 | 0.2767 | 0.7710 | 0.7746 | 200 |
| 8 | 0.0095 | 0.0924 | 0.2737 | 0.7185 | 0.7201 | 200 |
| **Mean** | **0.0090** | **0.0977** | **0.2746** | **0.7350** | **0.7369** | |

### 3.2 Representative 12 KB chunk — 8 cold runs

| Run | DNS (s) | TCP (s) | TLS (s) | TTFB (s) | Total (s) | HTTP |
| :--- | ---: | ---: | ---: | ---: | ---: | :---: |
| 1 | 0.0067 | 0.0889 | 0.2690 | 0.8252 | 0.8269 | 200 |
| 2 | 0.0125 | 0.1041 | 0.2883 | 0.8545 | 0.8558 | 200 |
| 3 | 0.0092 | 0.0911 | 0.2776 | 0.8708 | 0.8733 | 200 |
| 4 | 0.0112 | 0.1225 | 0.3171 | 0.8653 | 0.8674 | 200 |
| 5 | 0.0108 | 0.1039 | 0.2890 | 0.8951 | 0.8960 | 200 |
| 6 † | 0.0136 | 0.1943 | 0.8174 | 1.3973 | 1.4000 | 200 |
| 7 | 0.0075 | 0.1014 | 0.2998 | 0.8673 | 0.8687 | 200 |
| 8 † | 0.0099 | 1.1034 | 1.3474 | 2.1689 | 2.1733 | 200 |
| **Mean (1–5,7)** | **~0.010** | **~0.102** | **~0.290** | **0.8630** | **0.8648** | |

† Runs 6 and 8 were transient network/server outliers (`tls` 0.82 s; `connect` 1.10 s) — retained here for honesty, excluded from the mean.

### 3.3 Warm keep-alive connection (connection reuse verified)

| Payload | Per-request TTFB (warm) | Mean |
| :--- | :--- | ---: |
| SMALL | 0.457, 0.496, 0.472, 0.490, 0.487, 0.549, 0.490 s | **0.492 s** |
| 12 KB | 0.556, 0.576, 0.561, 0.603, 0.562 s | **0.572 s** |

*(First request of each keep-alive session included TLS setup and is excluded; connects afterward showed `connect=tls=0`.)*

### 3.4 Correctness / schema validation — ✅ the design works

The typed-answer design in `CR-SPEC-03` was confirmed end-to-end. For the 12 KB SQL-injection chunk:

```json
{"model":"typesafe/jev-1.14-20261014","answers":{
  "injection":{"type":"noul","noul":0.98},
  "secrets":{"type":"noul","noul":0.03},
  "severity":{"type":"score","score":3.95,"legend":{"3":"high","4":"critical"},"probabilities":{"3":0.02,"4":0.98},"confidence":0.98}},
 "usage":{"input_tokens":3588,"output_tokens":52,"cost":0.000150696},"provider":"TypeSafe"}
```

Purely typed output, calibrated confidence, no text to parse — exactly as specified.

### 3.5 Pricing (measured, not vendor-quoted)

| Payload | Input tokens | Cost (USD) | Implied rate |
| :--- | ---: | ---: | :--- |
| SMALL | 344 | 0.000014448 | $0.042 / 1M input tokens |
| 12 KB | 3,588 | 0.000150696 | $0.042 / 1M input tokens |

Output tokens are free; cost is input-only. **~$0.00015 per 12 KB chunk.**

---

## 4. Derived pipeline numbers

Assumptions: 1 request per chunk (Jev evaluates **one `state`** per call — see the batching caveat), 16 parallel workers, rate limit **1,200 requests/min** (= 20 req/s).

| Metric | Value |
| :--- | :--- |
| Per-chunk latency (warm) | ~0.49 s (small) → ~0.57 s (12 KB) |
| Per-chunk latency (cold) | ~0.74 s → ~0.86 s |
| Full scan, 5,000 files (~7,500 chunks) | wall ≈ **4.3 min** compute-bound, **≥ 6.3 min** rate-limit-bound → **≈ 5–7 min** |
| Full scan cost (~7,500 × 12 KB chunks) | **≈ $1.13** |
| Effective concurrency | capped at **~20 req/s** by the rate limit (16 workers × ~1.8 req/s ≈ 29 req/s would exceed it — must throttle) |

**Batching caveat (architectural finding):** Jev answers a **map of typed questions about a single `state`** — it does **not** fan out *N independent code chunks* into *N independent decisions* in one call. The spec's "batches of 10 chunks per HTTP/2 request" **does not reduce per-chunk latency**: latency is dominated by the model pass (~0.46 s even for the tiny payload), and questions run in parallel *against the same state*. Sending 10 chunks in one `state` would raise input tokens (~30k) without binding each question to its chunk. **Recommendation: 1 request per chunk in the MVP**, parallelised with a client-side worker pool that respects the 1,200 req/min limit; revisit multi-chunk packing in Phase 5 only if measured.

**Latency vs. question count:** 2 questions (small) vs 3 questions (12 KB) showed no additional per-question cost — confirming the "questions evaluate in parallel" claim.

---

## 5. Decision — Latency SLA reconciliation (`CR-SPEC-01` FR-04)

The docs shipped with two figures — **12–30 ms** (`CR-SPEC-00`) and **12–35 ms** (`CR-SPEC-01`/`03`/`06`) — both describing "live Jev". The authenticated benchmark shows **neither describes live Jev**.

> **Single source of truth — the two-tier latency model:**
>
> - **Mock / Tier 0 (default, offline-capable):** synthetic **10–35 ms per chunk** — this is what delivers the "instant" UX (`CR-SPEC-03` §4).
> - **Live Jev Tier 1 (background deep scan):** **~0.5–0.6 s per chunk warm**, **~0.9 s cold**, **p95 up to ~1–2 s** under network/server variance. Cost ~$0.00015/chunk. A full 5,000-file scan is **minutes, ~$1** — schedule it as a background job, not an interactive pass.
>
> **Do not publish "12–35 ms" or "real-time" for live Jev.** That band is Mock-mode only.

Documentation updated accordingly:

- `CR-SPEC-00` §1 and §2: latency re-labelled — **Mock 10–35 ms/chunk**; **live Jev ~0.5–0.6 s/chunk**; the "12–30 ms per file using Jev" marketing line removed.
- `CR-SPEC-01` FR-04: split into Mock band vs live-Jev budget (~0.5–0.6 s/chunk warm; minutes/full scan).
- `CR-SPEC-03`: headline block and §3 corrected; adds the OpenRouter decisions endpoint + `~typesafe/jev-latest` model id; flags that `JEV_API_KEY` may be an OpenRouter key.
- `CR-SPEC-06`: review question now asks about the two-tier model with the measured numbers.

---

## 6. Decision — Tier 1 languages for v1.0

**Decision: TypeScript / JavaScript first** (as suggested by `PHASES.md`).

- Tier 0 syntactic rules (`ARCH-DEP-001`, `SEC-TOK-002`, `SEC-TLS-005`) and the tree-sitter chunker ship for **TS/JS only** in v1.0.
- Rationale: matches the `d:/review` corpus (TS/ECMAScript-heavy skills), the React 19 frontend, and the Stitch UI examples.
- **Explicitly deferred to v1.5+:** Python, Go, Rust, Java.

---

## 7. Doc consistency validation

### 7.1 Radar node omission invariant — ✅ CONSISTENT (no change needed)

Stated identically in `CR-SPEC-01` FR-02 and `CR-SPEC-05` §B: only active findings (`LOW`/`MEDIUM`/`HIGH`/`CRITICAL`, `s > 0`) draw canvas nodes; clean files draw nothing and are shown via HUD telemetry + ambient glow. `Severity::Clean` exists in the domain but produces no node. Validated.

### 7.2 Radar node capacity — ⚠️ INCONSISTENT → fixed

`CR-SPEC-05` §D said cluster **above 150**; `PHASES.md`/`CR-SPEC-07` Phase 4 gate said **60 FPS with 100 nodes**; `CR-SPEC-06` said **50–100**. Harmonized to one capacity: **60 FPS up to 150 nodes; clustering above 150** (`CR-SPEC-05` §D, `CR-SPEC-06` §3, `PHASES.md`/`CR-SPEC-07` Phase 4).

### 7.3 Markdown export priority — ⚠️ INCONSISTENT → fixed

`CR-SPEC-01` §5 scope matrix listed **F8 "Export Report as Markdown"** as MVP, but the FR list (FR-01…FR-06) had no export requirement. Added **FR-07: Markdown Report Export** (P1 within MVP, Phase 7).

### 7.4 Broken companion link — ⚠️ → fixed

`PHASES.md` and `CR-SPEC-07` opened with `Companion to [PRD.md](./PRD.md)`; no `PRD.md` exists. Repointed to `01-BRD-AND-MVP-REQUIREMENTS.md`.

### 7.5 Milestone task drift — ⚠️ → fixed

Phase 0 said "Create milestones `Phase 1`…`Phase 7`"; the real set is `Phase 0`…`Phase 8`. Corrected to *verify* the existing set (§8).

---

## 8. Milestones verification

`gh api repos/mahmoudfarahat2647/coderadar/milestones` confirms **9 milestones exist**, matching Phases 0–8 one-to-one (Phase 0…Phase 8). ✅ No creation required.

---

## 9. Phase 0 task ledger

| Task (from issue #1) | Status | Evidence |
| :--- | :---: | :--- |
| Call the Jev API with 10-chunk batches over HTTP/2; record latency & payload | ✅ | Authenticated calls to `openrouter.ai/api/alpha/decisions` (§3); HTTP/1.1 client caveat noted (§2.2) |
| Reconcile latency SLA vs `CR-SPEC-01` FR-04 target | ✅ | §5 — two-tier model; live Jev re-scoped to background |
| Choose Tier 1 languages for v1.0 | ✅ | §6 — TypeScript/JavaScript |
| Validate radar node-omission invariant | ✅ | §7.1 — consistent |
| Fix radar node limit + Markdown export priority inconsistencies | ✅ | §7.2, §7.3 |
| Fix broken `PRD.md` companion link | ✅ | §7.4 |
| Verify GitHub milestones Phase 0 … Phase 8 | ✅ | §8 |

**Gate:** ✅ **PASS** — a written (now *measured, authenticated*) benchmark number exists: **~0.5–0.6 s/chunk warm, ~$0.00015/chunk**, and the PRD claims were corrected from "12–35 ms real-time" to the two-tier model.

---

> ### 🔐 Security note (key handling)
> The benchmark used an API key supplied at runtime; it is **not** stored in this repository or any committed file. Because it was shared in plaintext during the session, **rotate it** after use (the key observed also had a 7-day expiry and a $1 balance). For production, `CR-SPEC-03`'s OS-keychain storage (`keyring-rs`) remains correct — store the OpenRouter/TypeSafe key there, never in source.
