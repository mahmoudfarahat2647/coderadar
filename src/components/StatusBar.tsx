import React from "react";
import { MockModeStatus, ScanStats } from "../types";

interface StatusBarProps {
  mockStatus: MockModeStatus | null;
  scanStats: ScanStats | null;
  isScanning: boolean;
}

export const StatusBar: React.FC<StatusBarProps> = ({
  mockStatus,
  scanStats,
  isScanning,
}) => {
  return (
    <footer className="h-7 border-t border-border-subtle bg-surface-canvas px-4 flex items-center justify-between text-[11px] font-mono text-text-muted select-none z-20">
      {/* Left: Daemon & Ruleset & Mock Mode */}
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
          <span className="text-text-primary">{mockStatus?.ruleset_version || "v2026.1-strict"}</span>
        </div>

        <span className="text-border-strong hidden sm:inline">·</span>

        {/* Mock Mode Indicator */}
        <div
          className="flex items-center space-x-1 px-1.5 py-0.2 rounded bg-primary/10 border border-primary/25 text-secondary shrink-0 cursor-help"
          title={mockStatus?.description}
        >
          <span className="w-1.5 h-1.5 rounded-full bg-secondary" />
          <span className="font-semibold">Engine: Mock Mode</span>
          <span className="text-[10px] text-text-muted hidden lg:inline">
            ({mockStatus?.tier0_rules_count || 3} Tier 0 + {mockStatus?.mock_rules_count || 13} Mock)
          </span>
        </div>
      </div>

      {/* Right: Telemetry & Memory */}
      <div className="flex items-center space-x-3 shrink-0 tabular-nums">
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
