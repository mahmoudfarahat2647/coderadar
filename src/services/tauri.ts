/**
 * CodeRadar Studio — Tauri IPC Bridge Service.
 * Provides type-safe communication with the Rust backend.
 * Gracefully falls back to simulation mode in standard browser previews.
 */

import { EngineStatus, FindingBatch, ScanMode, ScanReport } from "../types";

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
