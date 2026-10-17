/**
 * CodeRadar Studio — Tauri IPC Bridge Service.
 * Provides type-safe communication with the Rust backend.
 * Gracefully falls back to simulation mode in standard browser previews.
 */

import { FindingBatch, MockModeStatus, ScanMode, ScanReport } from "../types";

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

export async function getMockModeStatus(): Promise<MockModeStatus> {
  if (isTauriEnvironment()) {
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      return await invoke<MockModeStatus>("get_mock_mode");
    } catch (e) {
      console.warn("Failed to get mock mode status from Tauri:", e);
    }
  }
  return {
    is_mock_mode: true,
    tier0_rules_count: 3,
    mock_rules_count: 13,
    total_rules: 19,
    ruleset_version: "v2026.1-strict",
    description:
      "Tier 0 (3 deterministic) + Mock Heuristics (13 regex) active. Jev Decision Engine integration in Phase 5.",
  };
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
