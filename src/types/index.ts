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
