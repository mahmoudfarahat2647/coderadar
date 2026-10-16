# 🎨 CodeRadar Studio: UI/UX & Polar Radar Canvas Specification

> **Document ID:** `CR-SPEC-05`  
> **Status:** Ready for Architectural Review  
> **Visual Reference:** Matches Stitch UI generated screen (`CodeRadar - Code & Security Radar`)  

---

## 1. Visual Hierarchy & Screen Sections

```
+--------------------------------------------------------------------------------------------------+
| [Logo] CodeRadar   (Dashboard) (Architecture) (Security) (Scanning Radar*) (PRs) (Settings)   🔍 🔔 👤 |
+--------------------------------------------------------------------------------------------------+
|                                                                                                  |
|  Code & Security          [ SCAN COVERAGE ]       [ CLEAN ARCHITECTURE ]    [ VULNERABILITIES ]  |
|  Radar                    Screened by Radar       Verified & Sound          Under Threshold      |
|                           98.4% ↗                 94.2% ↗                   0.08% ↗              |
|                                                                                                  |
|  (All) (Active Scan*) (Security) (Dependencies) (Edge Cases) (Auto-Fixed)     [Full Review] 📤 ⚙️ |
|                                                                                                  |
|  Analyzed: 120,090 LOC | Under Review: 2,311 | Resolved: 1,380 | Passed: 99.2% | Blocked: 230    |
+--------------------------------------------------------------------------------------------------+
|                                                                                                  |
|  +---------------------------+  (ALL*) (CRITICAL) (HIGH) (MEDIUM) (LOW)                          |
|  |           90 LOC/s        |                                                                   |
|  |               |           |  🔴 src/auth/jwt_verifier.ts [CRITICAL] [CWE-347]             >   |
|  |  45 LOC/s     |           |     Unvalidated RSA key header algorithm injection vulnerability  |
|  |       \       |  auth.ts  |                                                                   |
|  |        (·)----|---(clean) |  🟠 services/order/payment.py [HIGH] [CWE-798]                 >   |
|  |               |           |     Hardcoded API secret token exposure detected in commit history|
|  |   order.ts    |           |                                                                   |
|  |   (leak)      |payment.py |  🟡 pkg/network/router.go [MEDIUM] [CWE-778]                   >   |
|  |               |   (ok)    |     Missing rate limiter middleware on public webhook handler     |
|  |   360° Real-time Radar    |                                                                   |
|  +---------------------------+  🔵 config/helm/ingress.yaml [LOW] [CWE-326]                   >   |
|                                    Permissive TLS cipher suite version 1.1 allowed in gateway    |
+--------------------------------------------------------------------------------------------------+
| 🟢 Radar Daemon: Active 360° Sweep · Ruleset: v2026.1-strict · Latency: 12ms · Heap RSS: 38MB     |
+--------------------------------------------------------------------------------------------------+
```

---

## 2. Mathematical Specification of the 360° Polar Radar Canvas

The Radar Widget renders an HTML5 2D Canvas context drawing continuous polar sweeps and node coordinates:

### A. Coordinate Transformation & Radius Geometry
Let the radar center be $(x_0, y_0)$ and maximum radius be $R_{\max} = \min(w, h) / 2 - 16\text{px}$.

For any finding $F$ with risk score $s \in (0.0, 1.0]$:
$$x = x_0 + r \cdot \cos\left(\theta \cdot \frac{\pi}{180}\right)$$
$$y = y_0 - r \cdot \sin\left(\theta \cdot \frac{\pi}{180}\right)$$
Where the normalized radius $r$ places severe hazards toward the perimeter:
$$r = R_{\max} \cdot \left(0.20 + 0.80 \cdot s^{0.8}\right)$$

> **Canvas Coordinate Invariant ($Y$-Axis Inversion):** In HTML5 Canvas, the screen $Y$-axis increases downwards ($0,0$ is top-left). Subtracting $r \cdot \sin\theta$ ($y_0 - r \sin\theta$) is mathematically required so that $0^\circ - 90^\circ$ (positive $\sin$) maps to the visual **Top-Right** quadrant of the screen.

> **Geometric & Perceptual Rationale:** Placing high-risk findings toward the perimeter ($r \to R_{\max}$) maximizes available visual circumference ($C = 2\pi R_{\max}$). This provides the largest possible surface area for distinct nodes, hover interactions, and connecting vector beams without collision.

### B. Clean Node Omission Invariant
* **Crucial Performance & Visual Rule:** Files that pass inspection cleanly ($s = 0$) are **NEVER** drawn as individual canvas nodes. In a 5,000-file project, drawing 4,800 green dots near the center ($r \approx 0.15$) causes severe visual clutter, unreadable overlapping, and frame drops.
* **How Clean Status is Visualized:** Clean health is visualized via the central HUD telemetry metric (e.g. `98.4% Clean Passed`), live ribbon stats, and a subtle, periodic emerald ambient glow across the radar disc. Only actionable findings ($s > 0$) appear as interactive nodes.

### C. Quadrant Distribution ($\theta$) & Deterministic Angular Dispersion
The 360° canvas maps the 4 static codebase pillars into distinct quadrants:
1. **$0^\circ - 90^\circ$ (Top-Right):** Clean Architecture & Structural Integrity (Center: $45^\circ$).
2. **$90^\circ - 180^\circ$ (Top-Left):** Security & CWE Vulnerabilities (Center: $135^\circ$).
3. **$180^\circ - 270^\circ$ (Bottom-Left):** Performance & Resource Leaks (Center: $225^\circ$).
4. **$270^\circ - 360^\circ$ (Bottom-Right):** Edge Cases & Robustness (Center: $315^\circ$).

> **Pillar 5 (Pull Requests) Handling:** PR hygiene is an operational/diff dimension rather than a static codebase quadrant. When switching to **PR Scan Mode**, the radar projects only diff-modified files across the 4 quadrants, while a dedicated PR Hygiene Scorecard HUD displays scope, test coverage, and breaking change metrics.

#### Deterministic Angular Hash Dispersion Formula:
To eliminate node collision when multiple findings share identical severity within the same quadrant, $\theta$ is dispersed using a stable deterministic hash (FNV-1a) of the file path and line start:
$$\Delta\theta_{\text{hash}} = \left[\frac{\text{fnv1a}(\text{file\_path} + \text{line\_start}) \pmod{1000} - 500}{500.0}\right] \cdot 35^\circ$$
$$\theta = \theta_{\text{quadrant\_center}} + \Delta\theta_{\text{hash}}$$
This guarantees that all nodes within a quadrant remain inside their designated $90^\circ$ sector (with a $10^\circ$ safety buffer between quadrants) and never overlap.

### D. Sweeping Needle Physics & Dual-Layer Canvas Architecture
To maintain a rock-solid 60 FPS without triggering React 19 reconciliation overhead:
* **Decoupled Animation:** The radar needle angle is **never stored in React State (`useState`)**. It is animated purely through a `requestAnimationFrame` loop updating an HTML5 `<canvas>` via `useRef`.
* **Dual-Layer Canvas Structure:**
  1. **Background Layer (`<canvas id="radar-nodes">`):** Renders the circular grid rings, sector dividers, and finding nodes. This layer is static and only re-drawn when findings arrive via IPC or when active filters change.
  2. **Foreground Layer (`<canvas id="radar-sweep">`):** Renders only the rotating needle ($\omega = 72^\circ / \text{sec}$, full turn in 5s) and its phosphorescent decay trail (`rgba(234, 179, 8, alpha)` fading over $30^\circ$). Clearing and re-drawing this layer takes $< 0.5\text{ms}$ per frame.
* **HiDPI / Retina Crispness:** Both canvases scale their internal coordinate buffer by `window.devicePixelRatio` with CSS width/height fixed to maintain crisp rendering on 4K and Retina displays without blurring.
* **Accessibility (`prefers-reduced-motion`):** If the operating system has reduced motion enabled, the rotating needle animation is stopped, and the canvas displays a subtle static phosphorescent sweep line.
* **Node Capacity & Clustering Threshold:** The 60 FPS rendering target holds for **up to 150 on-screen nodes**. When total findings on screen exceed 150 nodes, close proximity nodes are clustered into small aggregate badges (e.g., `🔴 (4)`) to maintain interactive frame rates and prevent tap/click ambiguity. (This is the single capacity figure referenced by the Phase 4 gate — see [`CR-SPEC-08`](./08-PHASE-0-SPIKE-AND-DECISIONS.md) §5.2.)

### E. Node Styling by Severity:
* **Low Risk ($0 < s \le 0.3$):** `#38bdf8` (Sky Blue) - 4px radius.
* **Medium Risk ($0.3 < s \le 0.7$):** `#eab308` (Amber Gold) - 6px radius.
* **Critical / High ($s > 0.7$):** `#ef4444` (Vivid Crimson) - 8px radius with pulsating halo.

---

## 3. Interactive Issue Findings Feed & Diff Drawer

Each row in the findings feed displays verified line numbers or chunk spans:
* **Line Number Accuracy:** Findings detected via Tier 0 exact AST matching show exact line numbers (`src/auth/jwt_verifier.ts:42`). Findings surfaced via semantic chunk evaluation honestly display the analyzed function span (`src/auth/jwt_verifier.ts:35-85`).
* **Interaction Layers:**
  1. **Default State:** Shows file path, severity tag, CWE pill, rule summary, and chevron `>`.
  2. **Hover State:** Highlights corresponding node on the Polar Radar canvas with a connecting vector beam.
  3. **Selected / Click State:** Opens the **Tier 2 Diff & Remediation Modal**:
     ```
     +---------------------------------------------------------------+
     | 🔴 Finding Detail: CWE-347 in src/auth/jwt_verifier.ts:35-85  |
     +---------------------------------------------------------------+
     | Rule: Unvalidated RSA key header algorithm injection          |
     | Skill Source: Alibaba Open Code Review (Security Rule)        |
     |                                                               |
     | [-] const decoded = jwt.decode(token, { complete: true });    |
     | [-] if (decoded.header.alg === 'none') { ... }                |
     | [+] const verified = jwt.verify(token, pubKey, {              |
     | [+]   algorithms: ['RS256']                                   |
     | [+] });                                                       |
     |                                                               |
     | [ 📋 Copy Patch ]   [ 💾 Apply Fix to File ]   [ Dismiss ]    |
     +---------------------------------------------------------------+
     ```

---

## 4. Theme & Styling Invariants

* **Dark-Mode First:** Deep obsidian backdrop (`#0d0f12`), dark elevated surface cards (`#16191f`), crisp borders (`#222730`).
* **Semantic Accents:**
  - Active Radar Scan / Focus: Electric Gold / Lime (`#eab308` / `#84cc16`).
  - Critical Risk: Crimson (`#ef4444`).
  - Clean Pass: Emerald (`#10b981`).
* **Zero Hardcoded CSS:** All styling parameterized via Tailwind CSS classes and CSS variables, allowing complete plug-and-play with external `design.md` files.
