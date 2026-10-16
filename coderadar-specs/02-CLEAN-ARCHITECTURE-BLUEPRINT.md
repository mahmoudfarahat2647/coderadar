# 🏛️ CodeRadar Studio: Clean Architecture Blueprint & System Design

> **Document ID:** `CR-SPEC-02`  
> **Status:** Ready for Architectural Review  
> **Governing Philosophy:** Uncle Bob's Clean Architecture & Strict Dependency Rule  

---

## 1. Architectural Principles & Invariant Rules

### The Golden Rule: The Dependency Rule
* **Source code dependencies must point only inward:** Dependencies must refer only to higher-level abstractions and enterprise policies.
* **Strict Isolation of Inner Circles:** Nothing in an inner circle can know anything at all about something in an outer circle. The names of functions, structs, classes, variables, or data formats declared in an outer circle must never be mentioned by code in an inner circle.

```mermaid
graph TD
    subgraph Layer4["4. Frameworks & Drivers (Outer Details)"]
        Tauri["Tauri 2.0 Webview Window"]
        FileSystem["OS File System IO"]
    subgraph Layer4["4. Frameworks & Drivers (Outer Details)"]
        Tauri["Tauri 2.0 Webview Window"]
        FileSystem["OS File System IO"]
        JevAPI["Jev Cloud Client / Local Mock Driver"]
        SQLiteDB["SQLite Native Driver (sqlx)"]
    end

    subgraph Layer3["3. Interface Adapters"]
        Controllers["ScanController / FilterController"]
        Presenters["RadarPresenter / FindingPresenter"]
        Gateways["RuleEvaluatorAdapter / HistoryRepoImpl"]
        ViewModel["PolarCoordinate (Presentation Model)"]
    end

    subgraph Layer2["2. Application Business Rules (Use Cases)"]
        UC1["ExecuteRadarScanUseCase"]
        UC2["EvaluateFileRulesUseCase"]
        UC3["ComputeRadarMetricsUseCase"]
        UC4["ApplyCodePatchUseCase"]
    end

    subgraph Layer1["1. Enterprise Business Rules (Domain Core)"]
        E1["Finding (Entity)"]
        E2["RuleSpecification (Entity)"]
        E3["RiskScore (Value Object)"]
        E4["QualityScore (Value Object)"]
        E5["Severity & Category (Enums)"]
    end

    Layer4 --> Layer3
    Layer3 --> Layer2
    Layer2 --> Layer1
```

---

## 2. Layer-by-Layer Technical Specification

### Layer 1: Domain Entities (Core Inner Policy)
* **Purity Invariant:** Has zero dependencies on any external crate, UI framework, serialization library, or database driver (Pure Rust standard library types only).
* **Core Entities:**
  - `Finding`: Represents an identified defect or architectural violation. Contains purely domain business semantics with zero presentation coordinates.
    ```rust
    pub struct Finding {
        pub id: String,
        pub file_path: String,
        pub line_start: u32,
        pub line_end: u32,
        pub rule_id: String,
        pub category: FindingCategory,
        pub severity: Severity,
        pub cwe: Option<String>,
        pub title: String,
        pub description: String,
        pub confidence: f32,
        pub risk_score: f32, // Normalized domain risk score in [0.0, 1.0]
    }
    ```
  - `RiskScore`: Normalized domain risk score value object ($0.0$ = benign, $1.0$ = critical hazard).
  - `Severity`: Strongly-typed enum (`Critical`, `High`, `Medium`, `Low`, `Clean`).
  - `FindingCategory`: Strongly-typed enum (`CleanArchitecture`, `Security`, `EdgeCases`, `Performance`, `PullRequests`).

### Layer 2: Use Cases (Application Workflow)
* Orchestrates data flow to and from entities, executing specific business operations.
* Communicates with external services strictly through **Vendor-Agnostic Ports (Rust Traits)**.
* **Framework-Independence Invariant:** `crates/coderadar-usecases` has **zero dependencies on `tokio` or async runtime libraries**. All asynchronous interfaces return standard `std::future::Future` trait objects or generic callback sinks.
* **Trait Definitions (Boundary Ports):**
  ```rust
  use std::future::Future;
  use std::pin::Pin;

  pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

  /// Vendor-agnostic semantic assertion evaluator port (Layer 2)
  pub trait SemanticRuleEvaluator: Send + Sync {
      fn evaluate_batch<'a>(
          &'a self,
          chunks: Vec<CodeChunk>,
          rules: &'a [RuleSpec],
      ) -> BoxFuture<'a, Result<Vec<RuleEvaluationResult>, DomainError>>;
  }

  /// Trait for streaming chunks to decouple from concrete channels or runtimes
  pub trait ChunkSink: Send + Sync {
      fn send_chunk(&self, chunk: CodeChunk) -> Result<(), DomainError>;
  }

  pub trait ProjectScannerGateway: Send + Sync {
      fn stream_project_chunks(
          &self,
          root_dir: &str,
          mode: ScanMode,
          sink: &(dyn ChunkSink + 'static),
      ) -> Result<(), DomainError>;
  }

  pub trait ScanHistoryRepository: Send + Sync {
      fn save_scan_session<'a>(&'a self, session: &'a ScanSession) -> BoxFuture<'a, Result<(), DomainError>>;
      fn get_history<'a>(&'a self, limit: usize) -> BoxFuture<'a, Result<Vec<ScanSessionSummary>, DomainError>>;
  }
  ```

### Layer 3: Interface Adapters
* Translates data between the format most convenient for use cases/entities and the format most convenient for external agents (Webview UI, SQLite, HTTP).
* **Radar Presenter & Presentation Models:** Converts domain `Finding` entities into display-ready models without polluting the inner domain core.
  ```rust
  pub struct PolarCoordinate {
      pub radius: f32,    // 0.0 (safe baseline) to 1.0 (perimeter boundary)
      pub angle_deg: f32, // Deterministically hashed angle within category quadrant
  }

  pub struct RadarPresenter;
  impl RadarPresenter {
      pub fn project_to_polar(finding: &Finding) -> PolarCoordinate {
          let base_angle = match finding.category {
              FindingCategory::CleanArchitecture => 45.0,
              FindingCategory::Security => 135.0,
              FindingCategory::Performance => 225.0,
              FindingCategory::EdgeCases => 315.0,
              FindingCategory::PullRequests => 0.0, // Handled in dedicated PR Diff mode
          };
          // Apply deterministic hash dispersion (±35.0°) to prevent node overlap
          let hash_offset = compute_angle_jitter(&finding.file_path, finding.line_start, 35.0);
          PolarCoordinate {
              radius: finding.risk_score,
              angle_deg: (base_angle + hash_offset).rem_euclid(360.0),
          }
      }
  }
  ```
* **Data Transfer Objects (DTOs):** Inner entities are never serialized directly across the boundary. DTOs are lightweight, simple structs:
  ```rust
  #[derive(Serialize, Deserialize)]
  pub struct FindingDTO {
      pub id: String,
      pub file: String,
      pub line_start: u32,
      pub line_end: u32,
      pub rule_id: String,
      pub severity: String,
      pub cwe: Option<String>,
      pub title: String,
      pub radar_r: f32,
      pub radar_theta: f32,
  }
  ```

### Layer 4: Frameworks & Drivers
* Contains concrete implementations:
  - `JevRuleEvaluatorAdapter`: Implements `SemanticRuleEvaluator` by calling the Jev HTTP endpoint via `reqwest` connection pool.
  - `ParallelFileScanner`: Implements `ProjectScannerGateway` using `ignore::WalkParallel` and `tokio` thread pool.
  - `SqliteHistoryRepo`: Implements `ScanHistoryRepository` using asynchronous `sqlx`.
  - Tauri command handlers and Webview window management.

---

## 3. Physical Boundary Enforcement: Cargo Workspace Architecture

To mathematically guarantee Uncle Bob's Dependency Rule at compile time, the codebase is partitioned into distinct crates within a **Cargo Workspace**:

```
coderadar/
├── Cargo.toml (Workspace Root)
├── crates/
│   ├── coderadar-domain/          # Layer 1: std ONLY (No external dependencies)
│   ├── coderadar-usecases/        # Layer 2: Depends ONLY on coderadar-domain (No tokio, No reqwest)
│   ├── coderadar-adapters/        # Layer 3: Depends on usecases + domain (Presenters, DTOs)
│   └── coderadar-infrastructure/  # Layer 4: Depends on adapters + usecases + domain (Tauri/Tokio/Sqlx/Reqwest)
```

**Compiler Enforcement Invariant:** If any developer or AI assistant attempts to import `coderadar-adapters` inside `coderadar-domain`, or `tokio`/`reqwest` inside `coderadar-usecases`, the Rust compiler (`rustc`) immediately fails the build. Circular and outward dependencies are physically impossible.

---

## 4. Configurable Project Layer Mapping (`.coderadar.json`)

To evaluate Clean Architecture rules (`ARCH-DEP-001` Dependency Rule Violations and `ARCH-BND-004` Domain Boundary Leakage) deterministically, CodeRadar detects project architectural boundaries via an optional `.coderadar.json` file in the scanned repository root, or falls back to standard conventional heuristics:

```json
{
  "architecture_pattern": "clean_architecture",
  "layers": {
    "domain": ["src/domain/**", "lib/core/**", "domain/**"],
    "usecases": ["src/usecases/**", "src/application/**", "usecases/**"],
    "adapters": ["src/adapters/**", "src/controllers/**", "src/presenters/**"],
    "infrastructure": ["src/infra/**", "src/infrastructure/**", "src/database/**"]
  }
}
```

---

## 5. SOLID & Component Principles Matrix

| Principle | Architectural Enforcement in CodeRadar |
| :--- | :--- |
| **Single Responsibility (SRP)** | `ProjectScannerGateway` only reads/streams ASTs; `SemanticRuleEvaluator` only evaluates assertions; `RadarPresenter` computes UI coordinates; `Finding` only models business rules. |
| **Open-Closed (OCP)** | Adding a new review domain (e.g. `Accessibility`) requires adding a new `RuleSpec` schema without modifying the evaluation use case. |
| **Liskov Substitution (LSP)** | `MockRuleEvaluator` and `CloudJevRuleEvaluator` implement `SemanticRuleEvaluator` and are 100% interchangeable in tests and offline mode. |
| **Interface Segregation (ISP)** | Gateways are narrow and specialized (`PatchApplierGateway` is separate from `ProjectScannerGateway`). |
| **Dependency Inversion (DIP)** | Use cases depend exclusively on traits defined in the use case layer; concrete implementations in Layer 4 implement those traits. |
| **Acyclic Dependencies (ADP)** | Zero cyclic dependencies in the Rust crate graph; strictly validated by Cargo workspace dependency graph. |
| **Stable Abstractions (SAP)** | Core domain and trait definitions are purely abstract and highly stable; concrete drivers are unstable and easily swappable. |
