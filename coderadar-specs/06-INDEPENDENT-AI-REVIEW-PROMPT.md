# 🤖 Independent AI Architectural Review Prompt

> **Purpose:** Use this prompt when feeding the CodeRadar specification package into an external AI model (e.g., Claude 3.7 Sonnet, GPT-4o, o3-mini, or Gemini 2.0 Pro) to obtain an objective, rigorous architectural critique before writing code.

---

### Copy & Paste the Prompt Below:

```markdown
You are acting as a Principal Systems Architect, Staff Security Engineer, and Clean Architecture Specialist.

I am preparing to build a high-performance desktop application called "CodeRadar Studio" using Tauri 2.0 (Rust backend), React 19 + Tailwind CSS (frontend), and the Jev Decision Engine (by TypeSafe AI) for ultra-fast, zero-token-generation code reviews.

I have provided you with the complete architectural specification package:
1. CR-SPEC-00: Executive Summary & System Vision
2. CR-SPEC-01: Business Requirements Document (BRD) & MVP Scope
3. CR-SPEC-02: Clean Architecture Blueprint & Strict Dependency Rule Enforcement
4. CR-SPEC-03: Jev Decision Engine Integration & Dual-Tier Pipeline
5. CR-SPEC-04: Unified Review Skills Matrix & Deduplication Strategy
6. CR-SPEC-05: UI/UX & 360° Polar Radar Canvas Specification

Please conduct a thorough, unbiased, and adversarial architectural review of this system. Evaluate the design across the following dimensions:

### 1. Clean Architecture & Dependency Direction Compliance
- Does the system strictly enforce Uncle Bob's Dependency Rule (inward pointing dependencies)?
- Are Domain Entities completely isolated from frameworks, UI, databases, and external APIs?
- Are the interface adapters, ports (traits), and DTO boundaries properly segregated?
- Are there any subtle violations of SOLID principles (especially SRP, DIP, or boundary leakage)?

### 2. Jev Decision Model Integration & Latency Feasibility
- Is the Two-Tier latency model realistic: **10–35 ms/chunk in Mock mode**, and **70–500 ms per batched request (~16–59 ms/chunk amortized)** against live Jev when batching AST chunks over HTTP/IPC? (See CR-SPEC-08 for the measured Phase 0 numbers.)
- Are the proposed JSON schemas for Jev properly constrained to Booleans, Enums, and Floats?
- Is the Dual-Tier pipeline (Tier 1: Jev Decision -> Tier 2: On-Demand LLM Auto-Fix) sound and cost-effective?
- How resilient is the fallback mechanism (Built-in Local Mock Server)?

### 3. Real-Time Radar Canvas Performance
- Can an HTML5 Canvas or WebGL context handle continuous 360° sweeps and up to 150 animated nodes at 60 FPS (clustering activates above 150) without dropping frames or leaking memory?
- Is the polar-to-cartesian coordinate mapping formula mathematically sound and perceptually clear?

### 4. Scalability & Edge Cases
- What happens when a user points CodeRadar at a massive monorepo (e.g., 5,000+ files or 500,000 LOC)?
- Are the file-filtering and batching strategies in Rust sufficient to prevent memory exhaustion?
- What are the primary single-points-of-failure or bottlenecks in this design?

### 5. Final Verdict & Concrete Recommendations
- Provide an Architectural Feasibility Score from 1 to 10.
- Highlight any Blind Spots, High-Risk Assumptions, or Architectural Smells you detected.
- Provide 3 to 5 concrete refinements to incorporate before initiating implementation.
```
