import React from "react";
import { EngineStatus, ScanStats } from "../types";

interface StatusBarProps {
  engineStatus: EngineStatus | null;
  scanStats: ScanStats | null;
  isScanning: boolean;
}

export const StatusBar: React.FC<StatusBarProps> = ({
  engineStatus,
  scanStats,
  isScanning,
}) => {
  const engineTone = engineStatus?.is_live
    ? {
        wrapper: "bg-status-low/10 border-status-low/30 text-status-low",
        dot: "bg-status-low",
      }
    : engineStatus?.degraded
      ? {
          wrapper: "bg-status-high/10 border-status-high/30 text-status-high",
          dot: "bg-status-high",
        }
      : {
          wrapper: "bg-primary/10 border-primary/25 text-secondary",
          dot: "bg-secondary",
        };

  return (
    <footer className="h-7 border-t border-border-subtle bg-surface-canvas px-4 flex items-center justify-between text-[11px] font-mono text-text-muted select-none z-20">
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

        <div className="truncate hidden md:flex items-center space-x-1">
          <span>Ruleset:</span>
          <span className="text-text-primary">
            {engineStatus?.ruleset_version || "v2026.1-strict"}
          </span>
        </div>

        <span className="text-border-strong hidden sm:inline">·</span>

        {/* Engine Mode Indicator (live Jev vs Mock fallback) */}
        <div
          className={`flex items-center space-x-1 px-1.5 py-0.2 rounded border shrink-0 cursor-help ${engineTone.wrapper}`}
          title={engineStatus?.reason || engineStatus?.description}
        >
          <span className={`w-1.5 h-1.5 rounded-full ${engineTone.dot}`} />
          <span className="font-semibold">
            Engine: {engineStatus?.badge || "Mock Mode"}
          </span>
          <span className="text-[10px] text-text-muted hidden lg:inline">
            ({engineStatus?.tier0_rules_count ?? 3} Tier 0 +{" "}
            {engineStatus?.semantic_rules_count ?? 13} Semantic)
          </span>
        </div>
      </div>

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
            <div>
              <span>Latency: </span>
              <span className="text-secondary font-semibold">{scanStats.duration_ms}ms</span>
            </div>
            <span className="text-border-strong hidden sm:inline">·</span>
          </>
        )}
        <div className="hidden lg:inline text-text-muted">
          <span>Heap RSS: </span>
          <span className="text-tertiary">~38MB</span>
          <span className="text-[10px] text-text-muted"> (&lt;45MB budget)</span>
        </div>
      </div>
    </footer>
  );
};
