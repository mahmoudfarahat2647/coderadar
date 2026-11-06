import React from "react";
import { FileCode, AlertOctagon, CheckCircle2, ShieldCheck, Ban } from "lucide-react";
import { ScanCoverage } from "../types";

interface MetricsRibbonProps {
  coverage: ScanCoverage | null;
  underReview: number;
  resolved: number;
  passedPercent: number | null;
  blockedCount: number;
  isScanning: boolean;
}

export const MetricsRibbon: React.FC<MetricsRibbonProps> = ({
  coverage,
  underReview,
  resolved,
  passedPercent,
  blockedCount,
  isScanning,
}) => {
  return (
    <div className="grid grid-cols-2 md:grid-cols-5 gap-3 p-4 bg-surface-panel/40 border-b border-border-subtle">
      {/* 1. Analyzed LOC */}
      <div className="bg-surface-elevated/70 border border-border-subtle rounded-lg p-3 relative overflow-hidden transition-all hover:border-border-strong">
        <div className="flex items-center justify-between text-text-muted text-xs font-medium mb-1">
          <span>Files analyzed</span>
          <FileCode className="w-3.5 h-3.5 text-tertiary" />
        </div>
        <div className="flex items-baseline space-x-1.5">
          <span className="font-headline font-bold text-xl text-text-primary tabular-nums">
            {coverage == null ? "Unknown" : coverage.analyzed.toLocaleString()}
          </span>
          <span className="text-[11px] font-mono text-text-muted">files</span>
        </div>
        <div className="text-[11px] text-text-muted mt-1 flex items-center space-x-1">
          <span
            className={`w-1.5 h-1.5 rounded-full ${
              isScanning ? "bg-tertiary animate-ping" : "bg-secondary"
            } inline-block`}
          />
          <span>{isScanning ? "Streaming findings..." : coverage ? `${coverage.supported} supported · ${coverage.discovered} discovered` : "Coverage not recorded"}</span>
        </div>
        {!isScanning && coverage && (coverage.incomplete > 0 || coverage.unreadable > 0 || coverage.oversized > 0 || coverage.invalid_encoding > 0 || coverage.unsupported > 0) && (
          <div className="text-[11px] text-status-medium mt-1">
            {coverage.incomplete} incomplete · {coverage.unreadable} unreadable · {coverage.oversized} oversized · {coverage.invalid_encoding} invalid encoding · {coverage.unsupported} unsupported
          </div>
        )}
      </div>

      {/* 2. Under Review */}
      <div className="bg-surface-elevated/70 border border-border-subtle rounded-lg p-3 relative overflow-hidden transition-all hover:border-border-strong">
        <div className="flex items-center justify-between text-text-muted text-xs font-medium mb-1">
          <span>Under Review</span>
          <AlertOctagon className="w-3.5 h-3.5 text-status-medium" />
        </div>
        <div className="flex items-baseline space-x-1.5">
          <span className="font-headline font-bold text-xl text-status-medium tabular-nums">
            {underReview.toLocaleString()}
          </span>
          <span className="text-[11px] font-mono text-text-muted">findings</span>
        </div>
        <div className="text-[11px] text-text-muted mt-1">
          {underReview === 0 ? "No findings awaiting triage" : "Awaiting triage"}
        </div>
      </div>

      {/* 3. Resolved */}
      <div className="bg-surface-elevated/70 border border-border-subtle rounded-lg p-3 relative overflow-hidden transition-all hover:border-border-strong">
        <div className="flex items-center justify-between text-text-muted text-xs font-medium mb-1">
          <span>Resolved</span>
          <CheckCircle2 className="w-3.5 h-3.5 text-status-clean" />
        </div>
        <div className="flex items-baseline space-x-1.5">
          <span className="font-headline font-bold text-xl text-status-clean tabular-nums">
            {resolved.toLocaleString()}
          </span>
          <span className="text-[11px] font-mono text-text-muted">fixed</span>
        </div>
        <div className="text-[11px] text-text-muted mt-1">Marked resolved</div>
      </div>

      {/* 4. Passed % */}
      <div className="bg-surface-elevated/70 border border-border-subtle rounded-lg p-3 relative overflow-hidden transition-all hover:border-border-strong">
        <div className="flex items-center justify-between text-text-muted text-xs font-medium mb-1">
          <span>Files without findings</span>
          <ShieldCheck className="w-3.5 h-3.5 text-secondary" />
        </div>
        <div className="flex items-baseline space-x-1.5">
          <span className="font-headline font-bold text-xl text-secondary tabular-nums">
            {passedPercent == null ? "Unavailable" : `${passedPercent.toFixed(1)}%`}
          </span>
        </div>
        <div className="text-[11px] text-text-muted mt-1">Share of analyzed files without reported findings</div>
      </div>

      {/* 5. Blocked */}
      <div className={`border rounded-lg p-3 relative overflow-hidden transition-all ${
        blockedCount > 0
          ? "bg-status-critical/10 border-status-critical/30 shadow-glow-critical"
          : "bg-surface-elevated/70 border-border-subtle"
      }`}>
        <div className="flex items-center justify-between text-text-muted text-xs font-medium mb-1">
          <span className={blockedCount > 0 ? "text-status-critical font-semibold" : ""}>
            Blocked (Crit/High)
          </span>
          <Ban className={`w-3.5 h-3.5 ${blockedCount > 0 ? "text-status-critical" : "text-text-muted"}`} />
        </div>
        <div className="flex items-baseline space-x-1.5">
          <span className={`font-headline font-bold text-xl tabular-nums ${
            blockedCount > 0 ? "text-status-critical" : "text-text-primary"
          }`}>
            {blockedCount}
          </span>
          <span className="text-[11px] font-mono text-text-muted">gate blockers</span>
        </div>
        <div className="text-[11px] text-text-muted mt-1">
          {blockedCount > 0 ? "Critical or high findings" : "No critical or high findings"}
        </div>
      </div>
    </div>
  );
};
