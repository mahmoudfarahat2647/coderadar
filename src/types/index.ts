/**
 * CodeRadar Studio TypeScript Type Definitions.
 * Strict Layer 3 / Layer 4 DTO models.
 */

export type Severity = "CRITICAL" | "HIGH" | "MEDIUM" | "LOW" | "CLEAN";

export type FindingCategory =
  | "architecture"
  | "security"
  | "edge_cases"
  | "performance"
  | "pull_requests";

export interface FindingDTO {
  id: string;
  file: string;
  line_start: number;
  line_end: number;
  rule_id: string;
  category: string;
  severity: Severity;
  cwe?: string | null;
  title: string;
  description: string;
  radar_r: number;
  radar_theta: number;
  resolved?: boolean;
}

export interface FindingBatch {
  findings: FindingDTO[];
  elapsed_ms: number;
  count: number;
  is_last: boolean;
}

export interface SeverityTotals {
  total: number;
  critical: number;
  high: number;
  medium: number;
  low: number;
}

export interface ScanStats {
  files_scanned: number;
  ast_chunks: number;
  module_chunks: number;
  duration_ms: number;
}

/**
 * Phase 8 - Pull Request hygiene scorecard (staged_diff scans only).
 * The backend omits `pr_hygiene` for full scans.
 */
export interface PrHygieneReport {
  base: string;
  files_changed: number;
  additions: number;
  deletions: number;
  lines_changed: number;
  production_files_changed: number;
  test_files_changed: number;
  areas_touched: string[];
  scope_creep: boolean;
  breaking_change: boolean;
  missing_tests: boolean;
}

export interface ScanReport {
  tool: {
    name: string;
    version: string;
  };
  ruleset_version: string;
  layer_map_source: string;
  scan: ScanStats;
  session: {
    id: string;
    root_dir: string;
    mode: string;
    started_at_unix_ms: number;
    finished_at_unix_ms?: number;
    totals: SeverityTotals;
  };
  findings: FindingDTO[];
  pr_hygiene?: PrHygieneReport | null;
}

export interface EngineStatus {
  mode: string;
  is_live: boolean;
  degraded: boolean;
  badge: string;
  reason?: string | null;
  tier1_requests: number;
  fallback_events: number;
  last_latency_ms: number;
  max_latency_ms: number;
  total_latency_ms: number;
  api_key_configured: boolean;
  api_key_masked?: string | null;
  tier0_rules_count: number;
  semantic_rules_count: number;
  total_rules: number;
  ruleset_version: string;
  description: string;
}

export type ScanMode = "full_scan" | "staged_diff";

export type FilterSeverity = "ALL" | "CRITICAL" | "HIGH" | "MEDIUM" | "LOW";

export type FilterCategory =
  | "ALL"
  | "architecture"
  | "security"
  | "edge_cases"
  | "performance"
  | "pull_requests";

/**
 * Phase 6 — Safe Atomic Patching (CR-SPEC-13).
 * A proposal carries the fingerprint of the original file so the backend can
 * refuse to write against a file that changed since the preview was produced.
 */
export type PatchStatus = "applied" | "conflict" | "rejected";

/**
 * One line edit of a patch. The backend re-applies the edits to the file on
 * disk and refuses the patch when the result does not match `new_content`.
 */
export interface LineEditDTO {
  start_line: number;
  end_line: number;
  start_offset: number;
  end_offset: number;
  replacement: string;
}

export interface PatchProposalDTO {
  file_path: string;
  rule_id: string;
  original_sha256: string;
  original_line_count: number;
  edits: LineEditDTO[];
  new_content: string;
  unified_diff: string;
  explanation: string;
}

export interface PatchPreviewDTO {
  file_path: string;
  rule_id: string;
  title: string;
  explanation: string;
  original_sha256: string;
  new_sha256: string;
  original_line_count: number;
  unified_diff: string;
  added_lines: number;
  removed_lines: number;
  warnings: string[];
  /**
   * The patch payload, or `null` when no deterministic fix exists for the
   * finding: the remediation template did not match the code exactly
   * (CR-SPEC-13 §5). A Tier 2 (LLM) fix is generated separately and arrives
   * as a full preview through `generate_fix`.
   */
  proposal: PatchProposalDTO | null;
}

export interface PatchOutcomeDTO {
  status: PatchStatus;
  file_path: string;
  message: string;
  original_sha256: string;
  new_sha256?: string | null;
}

/**
 * Phase 6 — SQLite scan history. The backend returns points newest-first;
 * consumers are responsible for re-ordering them chronologically.
 */
export interface HistoryPointDTO {
  id: string;
  root_dir: string;
  mode: string;
  finished_at_unix_ms: number;
  files_scanned: number;
  total_findings: number;
  critical: number;
  high: number;
  medium: number;
  low: number;
  compliance_percent: number;
}

/** Phase 7 — Markdown report export (FR-07). */
export interface ExportOutcomeDTO {
  path: string;
  bytes: number;
  findings: number;
}

/** Phase 8 - Tier 2 provider configuration (local Ollama / cloud Gemini). */
export type Tier2Provider = "ollama" | "gemini";

export interface Tier2StatusDTO {
  local_provider: string;
  local_model: string;
  local_endpoint: string;
  cloud_provider: string;
  cloud_model: string;
  cloud_key_configured: boolean;
}

export interface Tier2FixRequestDTO {
  root_path: string;
  file_path: string;
  rule_id: string;
  title: string;
  description: string;
  line_start: number;
  line_end: number;
  provider: Tier2Provider;
  /** Explicit consent before a fully redacted file leaves the machine. */
  allow_cloud: boolean;
}