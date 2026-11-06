import React from "react";
import { KeyRound } from "lucide-react";
import { EngineStatus, ScanProvenance, ScanReport, ScanStats } from "../types";
import { engineModeLabel, engineTone } from "../utils/engineTone";

interface StatusBarProps {
  engineStatus: EngineStatus | null;
  scanStats: ScanStats | null;
  session: ScanProvenance | null;
  historyStatus?: ScanReport["history_status"];
  isScanning: boolean;
  onOpenApiKey: () => void;
}

export const StatusBar: React.FC<StatusBarProps> = ({
  engineStatus,
  scanStats,
  session,
  historyStatus,
  isScanning,
  onOpenApiKey,
}) => {
  const tone = engineTone(engineStatus);

  return (
    <footer className="min-h-7 border-t border-border-subtle bg-surface-canvas px-4 flex flex-wrap items-center justify-between text-[11px] font-mono text-text-muted select-none z-20">
      {/* Left: Daemon & Ruleset & Engine Mode */}
      <div className="flex items-center space-x-3 truncate">
        <div className="flex items-center space-x-1.5 shrink-0">
          <span className="w-2 h-2 rounded-full bg-secondary animate-pulse" />
          <span className="text-text-primary font-medium">Radar Daemon:</span>
          <span className="text-secondary">
            {isScanning ? "Active 360° Sweep..." : "Active"}
          </span>
        </div>

        <span className="text-border-strong hidden sm:inline">·</span>

        {engineStatus && (
          <div className="truncate hidden md:flex items-center space-x-1">
            <span>Ruleset:</span>
            <span className="text-text-primary">{engineStatus.ruleset_version}</span>
          </div>
        )}

        <span className="text-border-strong hidden sm:inline">·</span>

        {/* Engine Mode Indicator (live Jev vs Mock fallback) */}
        <div
          className={`flex items-center space-x-1 px-1.5 py-0.2 rounded border shrink-0 cursor-help ${tone.wrapper}`}
          title={engineStatus?.reason || engineStatus?.description}
        >
          <span className={`w-1.5 h-1.5 rounded-full ${tone.dot}`} />
          <span className="font-semibold">Provider: {engineStatus?.badge || tone.label}</span>
          {engineStatus && (
            <span className="text-[10px] text-text-muted hidden lg:inline">
              ({engineStatus.tier0_rules_count} Tier 0 +{" "}
              {engineStatus.semantic_rules_count} Semantic)
            </span>
          )}
        </div>
      </div>

      {(historyStatus === "in_memory" || historyStatus === "save_failed") && (
        <div role="status" className="w-full order-last py-1 text-status-high">
          {historyStatus === "in_memory"
            ? "History is temporary and will be lost when the app closes. Export the report and check access to the history database, then restart the app."
            : "This scan was not saved to history. Export the report and check access to the history database before scanning again."}
        </div>
      )}
      {session && (
        <div className="w-full order-last py-1" title={session.usage
          ? `Primary batches: ${session.usage.primary_batches}; Fallback batches: ${session.usage.fallback_batches}; Fallback failures: ${session.usage.fallback_failures}`
          : "Usage not recorded"}>
          Scan: {engineModeLabel(session.engine_mode)}. {session.usage
            ? `Primary batches: ${session.usage.primary_batches}; Fallback batches: ${session.usage.fallback_batches}; Fallback failures: ${session.usage.fallback_failures}`
            : "Usage not recorded"}
        </div>
      )}

      {/* Right: Telemetry & Memory */}
      <div className="flex items-center space-x-3 shrink-0 tabular-nums">
        {engineStatus?.is_live && engineStatus.tier1_requests > 0 && (
          <>
            <div className="hidden lg:inline">
              <span>Jev: </span>
              <span className="text-status-low font-semibold">
                {engineStatus.last_latency_ms}ms / {engineStatus.max_latency_ms}ms max
              </span>
            </div>
            <span className="text-border-strong hidden lg:inline">·</span>
          </>
        )}
        {scanStats && (
          <>
            <div className="hidden sm:inline">
              <span>Files: </span>
              <span className="text-text-primary font-semibold">{scanStats.files_scanned}</span>
            </div>
            <span className="text-border-strong hidden sm:inline">·</span>
            {scanStats.suppressed_heuristic_findings > 0 && (
              <>
                <div
                  className="hidden lg:inline cursor-help"
                  title="Heuristic findings dropped by the confidence floor (kept out of the report)"
                >
                  <span>Suppressed: </span>
                  <span className="text-text-muted font-semibold">
                    {scanStats.suppressed_heuristic_findings}
                  </span>
                </div>
                <span className="text-border-strong hidden lg:inline">·</span>
              </>
            )}
            <div>
              <span>Latency: </span>
              <span className="text-secondary font-semibold">{scanStats.duration_ms}ms</span>
            </div>
            <span className="text-border-strong hidden sm:inline">·</span>
          </>
        )}
        <button
          onClick={onOpenApiKey}
          className="flex items-center space-x-1 px-1.5 py-0.5 rounded border border-border-subtle hover:border-border-strong hover:text-text-primary transition-colors"
          title="Configure the Jev API key (OS keychain)"
        >
          <KeyRound className="w-3 h-3" />
          <span>API Key</span>
        </button>
      </div>
    </footer>
  );
};
