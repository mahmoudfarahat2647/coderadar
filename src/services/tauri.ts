/**
 * CodeRadar Studio — Tauri IPC Bridge Service.
 * Provides type-safe communication with the Rust backend.
 * Gracefully falls back to simulation mode in standard browser previews.
 */

import {
  EngineStatus,
  ExportOutcomeDTO,
  FindingBatch,
  HistoryPointDTO,
  PatchOutcomeDTO,
  PatchPreviewDTO,
  PatchProposalDTO,
  ScanMode,
  ScanReport,
  Tier2FixRequestDTO,
  Tier2StatusDTO,
} from "../types";

export const isTauriEnvironment = (): boolean => {
  return (
    typeof window !== "undefined" &&
    ("__TAURI_INTERNALS__" in window || "__TAURI__" in window)
  );
};

export async function openFolderDialog(): Promise<string | null> {
  if (isTauriEnvironment()) {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({
        directory: true,
        multiple: false,
        title: "Select Project Directory to Scan",
      });
      if (typeof selected === "string") {
        return selected;
      }
      return null;
    } catch (err) {
      console.warn("Failed to open native folder dialog via Tauri:", err);
    }
  }

  // Web browser fallback: prompt user for folder path
  const fallback = window.prompt(
    "Enter project directory path to scan:",
    "tests/fixtures/known-bad-repo"
  );
  return fallback || null;
}

export async function getDefaultScanPath(): Promise<string> {
  if (isTauriEnvironment()) {
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      return await invoke<string>("get_default_scan_path");
    } catch (e) {
      console.warn("Failed to get default scan path from Tauri:", e);
    }
  }
  return "tests/fixtures/known-bad-repo";
}

const FALLBACK_ENGINE_STATUS: EngineStatus = {
  mode: "mock",
  is_live: false,
  degraded: false,
  badge: "Mock Mode",
  reason: null,
  tier1_requests: 0,
  fallback_events: 0,
  last_latency_ms: 0,
  max_latency_ms: 0,
  total_latency_ms: 0,
  api_key_configured: false,
  api_key_masked: null,
  tier0_rules_count: 3,
  semantic_rules_count: 13,
  total_rules: 19,
  ruleset_version: "v2026.1-strict",
  description:
    "Mock Mode: 3 Tier 0 (deterministic) + 13 semantic rules active. Configure an API key to enable live Jev Tier 1.",
};

export async function getEngineStatus(): Promise<EngineStatus> {
  if (isTauriEnvironment()) {
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      return await invoke<EngineStatus>("get_engine_status");
    } catch (e) {
      console.warn("Failed to get engine status from Tauri:", e);
    }
  }
  return FALLBACK_ENGINE_STATUS;
}

export async function storeApiKey(apiKey: string): Promise<EngineStatus> {
  if (isTauriEnvironment()) {
    const { invoke } = await import("@tauri-apps/api/core");
    return await invoke<EngineStatus>("store_api_key", { apiKey });
  }
  throw new Error("Tauri environment not detected. Please run within CodeRadar desktop app.");
}

export async function clearApiKey(): Promise<EngineStatus> {
  if (isTauriEnvironment()) {
    const { invoke } = await import("@tauri-apps/api/core");
    return await invoke<EngineStatus>("clear_api_key");
  }
  throw new Error("Tauri environment not detected. Please run within CodeRadar desktop app.");
}

export async function getRulesManifest(): Promise<unknown[]> {
  if (isTauriEnvironment()) {
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const rules = await invoke<unknown[]>("get_rules");
      return rules;
    } catch (e) {
      console.warn("Failed to get rules from Tauri:", e);
    }
  }
  return [];
}

export async function listenToScanBatches(
  onBatch: (batch: FindingBatch) => void,
  onComplete: (report: ScanReport) => void,
  onError: (error: string) => void
): Promise<() => void> {
  if (isTauriEnvironment()) {
    try {
      const { listen } = await import("@tauri-apps/api/event");
      const unlistenBatch = await listen<FindingBatch>("coderadar://scan-batch", (event) => {
        onBatch(event.payload);
      });
      const unlistenComplete = await listen<ScanReport>("coderadar://scan-complete", (event) => {
        onComplete(event.payload);
      });
      const unlistenError = await listen<string>("coderadar://scan-error", (event) => {
        onError(event.payload);
      });

      return () => {
        unlistenBatch();
        unlistenComplete();
        unlistenError();
      };
    } catch (err) {
      console.warn("Failed to attach Tauri event listeners:", err);
    }
  }
  return () => {};
}

export async function startScan(
  path: string,
  mode: ScanMode
): Promise<ScanReport> {
  if (isTauriEnvironment()) {
    const { invoke } = await import("@tauri-apps/api/core");
    return await invoke<ScanReport>("start_scan", { path, mode });
  }

  // Browser simulation: simulate finding stream for testing
  throw new Error(
    "Tauri environment not detected. Please run within CodeRadar desktop app."
  );
}

/**
 * Phase 6 — Request a safe remediation proposal for a single finding.
 *
 * The returned preview carries `proposal: null` when no deterministic fix
 * exists for the rule (CR-SPEC-13 §5); the modal explains the situation
 * instead of offering an apply action.
 *
 * Read-only: returns `null` in the browser preview where the Rust engine is absent.
 */
export async function previewPatch(
  rootPath: string,
  filePath: string,
  ruleId: string
): Promise<PatchPreviewDTO | null> {
  if (isTauriEnvironment()) {
    const { invoke } = await import("@tauri-apps/api/core");
    return await invoke<PatchPreviewDTO>("preview_patch", {
      rootPath,
      filePath,
      ruleId,
    });
  }
  return null;
}

/**
 * Phase 6 — Apply a previously previewed proposal atomically.
 * Mutating: returns `null` in the browser preview where the Rust engine is absent.
 */
export async function applyPatch(
  rootPath: string,
  proposal: PatchProposalDTO
): Promise<PatchOutcomeDTO | null> {
  if (isTauriEnvironment()) {
    const { invoke } = await import("@tauri-apps/api/core");
    return await invoke<PatchOutcomeDTO>("apply_patch", { rootPath, proposal });
  }
  return null;
}

/**
 * Phase 6 — Read the persisted SQLite scan history (newest first).
 * Read-only: degrades to an empty history in the browser preview.
 *
 * When `rootPath` is given, only the sessions of that workspace are returned,
 * so the trend never mixes projects.
 */
export async function getScanHistory(
  limit = 20,
  rootPath?: string
): Promise<HistoryPointDTO[]> {
  if (isTauriEnvironment()) {
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      return await invoke<HistoryPointDTO[]>("get_scan_history", {
        limit,
        rootPath: rootPath?.trim() || null,
      });
    } catch (e) {
      console.warn("Failed to read scan history from Tauri:", e);
    }
  }
  return [];
}

/**
 * Phase 7 — Write the report of the last scan as a Markdown document.
 *
 * Mutating: returns `null` in the browser preview where the Rust engine is
 * absent. A rejected export propagates its error so the UI can show it.
 */
export async function exportReport(
  targetPath: string
): Promise<ExportOutcomeDTO | null> {
  if (isTauriEnvironment()) {
    const { invoke } = await import("@tauri-apps/api/core");
    try {
      return await invoke<ExportOutcomeDTO>("export_report", { targetPath });
    } catch (e) {
      console.warn("Failed to export the Markdown report via Tauri:", e);
      throw e;
    }
  }
  return null;
}

/**
 * Phase 7 — Ask the OS where the Markdown report should be written.
 *
 * Returns `null` when the user cancels the dialog, and in a plain browser
 * where no native dialog is available.
 */
export async function pickReportPath(): Promise<string | null> {
  if (isTauriEnvironment()) {
    try {
      const { save } = await import("@tauri-apps/plugin-dialog");
      return await save({
        defaultPath: "report.md",
        title: "Export the scan report",
        filters: [{ name: "Markdown", extensions: ["md"] }],
      });
    } catch (err) {
      console.warn("Failed to open the native save dialog via Tauri:", err);
    }
  }
  return null;
}

/**
 * Phase 8 - Read the Tier 2 provider configuration (local Ollama / cloud Gemini).
 * Read-only: degrades to `null` in the browser preview.
 */
export async function getTier2Status(): Promise<Tier2StatusDTO | null> {
  if (isTauriEnvironment()) {
    const { invoke } = await import("@tauri-apps/api/core");
    return await invoke<Tier2StatusDTO>("get_tier2_status");
  }
  return null;
}

/** Phase 8 - Store the Gemini API key in the OS keychain. */
export async function storeTier2Key(
  apiKey: string
): Promise<Tier2StatusDTO | null> {
  if (isTauriEnvironment()) {
    const { invoke } = await import("@tauri-apps/api/core");
    return await invoke<Tier2StatusDTO>("store_tier2_key", { apiKey });
  }
  throw new Error("Tauri environment not detected. Please run within CodeRadar desktop app.");
}

/** Phase 8 - Remove the stored Gemini API key. */
export async function clearTier2Key(): Promise<Tier2StatusDTO | null> {
  if (isTauriEnvironment()) {
    const { invoke } = await import("@tauri-apps/api/core");
    return await invoke<Tier2StatusDTO>("clear_tier2_key");
  }
  throw new Error("Tauri environment not detected. Please run within CodeRadar desktop app.");
}

/**
 * Phase 8 - Ask a Tier 2 model for a fix and receive a reviewable diff preview.
 *
 * Nothing is written: the returned preview is applied through the same
 * workspace-guarded `apply_patch` path as a deterministic fix. Cloud providers
 * require `allow_cloud` consent and a fully redacted file.
 */
export async function generateFix(
  request: Tier2FixRequestDTO
): Promise<PatchPreviewDTO | null> {
  if (isTauriEnvironment()) {
    const { invoke } = await import("@tauri-apps/api/core");
    return await invoke<PatchPreviewDTO>("generate_fix", { request });
  }
  return null;
}
