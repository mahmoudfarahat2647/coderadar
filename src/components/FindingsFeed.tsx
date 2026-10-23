import React from "react";
import { ChevronRight, Shield, AlertTriangle, AlertCircle, Info, Check, Copy } from "lucide-react";
import { FindingDTO, Severity } from "../types";
import { EngineChip } from "./EngineChip";

interface FindingsFeedProps {
  findings: FindingDTO[];
  selectedFinding: FindingDTO | null;
  hoveredFindingId?: string | null;
  onSelectFinding: (finding: FindingDTO) => void;
  onHoverFinding?: (id: string | null) => void;
  onToggleResolve: (id: string, e: React.MouseEvent) => void;
  isScanning: boolean;
}

export const FindingsFeed: React.FC<FindingsFeedProps> = ({
  findings,
  selectedFinding,
  hoveredFindingId,
  onSelectFinding,
  onHoverFinding,
  onToggleResolve,
  isScanning,
}) => {
  const getSeverityBadge = (severity: Severity) => {
    switch (severity) {
      case "CRITICAL":
        return {
          icon: <AlertOctagonIcon className="w-3.5 h-3.5 text-status-critical" />,
          color: "text-status-critical bg-status-critical/10 border-status-critical/30",
          dot: "bg-status-critical",
        };
      case "HIGH":
        return {
          icon: <AlertTriangle className="w-3.5 h-3.5 text-status-high" />,
          color: "text-status-high bg-status-high/10 border-status-high/30",
          dot: "bg-status-high",
        };
      case "MEDIUM":
        return {
          icon: <AlertCircle className="w-3.5 h-3.5 text-status-medium" />,
          color: "text-status-medium bg-status-medium/10 border-status-medium/30",
          dot: "bg-status-medium",
        };
      case "LOW":
      default:
        return {
          icon: <Info className="w-3.5 h-3.5 text-status-low" />,
          color: "text-status-low bg-status-low/10 border-status-low/30",
          dot: "bg-status-low",
        };
    }
  };

  const copyPath = (path: string, e: React.MouseEvent) => {
    e.stopPropagation();
    navigator.clipboard.writeText(path);
  };

  if (findings.length === 0) {
    return (
      <div className="flex-1 flex flex-col items-center justify-center p-8 text-center select-none">
        <div className="w-14 h-14 rounded-full bg-surface-elevated border border-border-subtle flex items-center justify-center mb-3 text-secondary">
          <Shield className="w-7 h-7 text-secondary" />
        </div>
        <h3 className="font-headline font-semibold text-base text-text-primary mb-1">
          {isScanning ? "Scanning Project..." : "No Findings in Current View"}
        </h3>
        <p className="text-text-muted text-xs max-w-sm">
          {isScanning
            ? "Evaluating code chunks in bounded batches via Tier 0 & Mock engine..."
            : "Either this project passed clean or current filters exclude all findings."}
        </p>
      </div>
    );
  }

  return (
    <div className="flex-1 overflow-y-auto divide-y divide-border-subtle select-none">
      {findings.map((finding) => {
        const badge = getSeverityBadge(finding.severity);
        const isSelected = selectedFinding?.id === finding.id;
        const lineSpan =
          finding.line_start === finding.line_end
            ? `:${finding.line_start}`
            : `:${finding.line_start}-${finding.line_end}`;

        const isHovered = hoveredFindingId === finding.id;

        return (
          <div
            key={finding.id}
            onClick={() => onSelectFinding(finding)}
            onMouseEnter={() => onHoverFinding?.(finding.id)}
            onMouseLeave={() => onHoverFinding?.(null)}
            className={`p-3.5 transition-all cursor-pointer group flex items-start justify-between gap-4 ${
              isSelected
                ? "bg-surface-elevated/90 border-l-2 border-l-secondary shadow-md"
                : isHovered
                ? "bg-surface-elevated/70 border-l-2 border-l-tertiary"
                : "hover:bg-surface-elevated/40"
            } ${finding.resolved ? "opacity-60 bg-surface-canvas/40" : ""}`}
          >
            {/* Left: Severity indicator and finding summary */}
            <div className="flex-1 min-w-0">
              <div className="flex items-center flex-wrap gap-2 mb-1.5">
                {/* Severity Badge */}
                <span
                  className={`inline-flex items-center space-x-1 px-2 py-0.5 rounded text-[10px] font-mono font-semibold uppercase border ${badge.color}`}
                >
                  <span className={`w-1.5 h-1.5 rounded-full ${badge.dot}`} />
                  <span>{finding.severity}</span>
                </span>

                {/* CWE Badge if present */}
                {finding.cwe && (
                  <span className="inline-flex items-center px-1.5 py-0.5 rounded text-[10px] font-mono text-tertiary bg-tertiary/10 border border-tertiary/25">
                    {finding.cwe}
                  </span>
                )}

                {/* Rule ID */}
                <span className="inline-flex items-center px-1.5 py-0.5 rounded text-[10px] font-mono text-text-muted bg-surface-panel border border-border-subtle">
                  {finding.rule_id}
                </span>

                {/* Engine origin + confidence (precision markers) */}
                <EngineChip finding={finding} />

                {/* File Path & Line Span */}
                <div className="flex items-center space-x-1 font-mono text-xs font-semibold text-text-primary group-hover:text-secondary transition-colors truncate">
                  <span className="truncate">{finding.file}</span>
                  <span className="text-secondary shrink-0">{lineSpan}</span>
                  <button
                    onClick={(e) => copyPath(`${finding.file}${lineSpan}`, e)}
                    className="p-1 text-text-muted hover:text-text-primary opacity-0 group-hover:opacity-100 transition-opacity"
                    title="Copy file path and line span"
                  >
                    <Copy className="w-3 h-3" />
                  </button>
                </div>
              </div>

              {/* Finding Title & Description */}
              <div className="text-xs font-medium text-text-primary mb-1">
                {finding.title || finding.rule_id}
              </div>
              <p className="text-[11px] text-text-secondary line-clamp-2 leading-relaxed">
                {finding.description}
              </p>
            </div>

            {/* Right: Actions & Chevron */}
            <div className="flex items-center space-x-2 shrink-0 pt-1">
              <button
                onClick={(e) => onToggleResolve(finding.id, e)}
                className={`p-1.5 rounded-md border text-xs transition-colors ${
                  finding.resolved
                    ? "bg-status-clean/15 border-status-clean/40 text-status-clean"
                    : "bg-surface-elevated border-border-subtle text-text-muted hover:text-text-primary hover:border-border-strong"
                }`}
                title={finding.resolved ? "Mark as unresolved" : "Mark as resolved"}
              >
                <Check className="w-3.5 h-3.5" />
              </button>

              <ChevronRight
                className={`w-4 h-4 text-text-muted transition-transform group-hover:translate-x-0.5 ${
                  isSelected ? "text-secondary transform rotate-90" : ""
                }`}
              />
            </div>
          </div>
        );
      })}
    </div>
  );
};

const AlertOctagonIcon: React.FC<{ className?: string }> = ({ className }) => (
  <svg
    className={className}
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth="2"
    strokeLinecap="round"
    strokeLinejoin="round"
  >
    <polygon points="7.86 2 16.14 2 22 7.86 22 16.14 16.14 22 7.86 22 2 16.14 2 7.86 7.86 2" />
    <line x1="12" y1="8" x2="12" y2="12" />
    <line x1="12" y1="16" x2="12.01" y2="16" />
  </svg>
);
