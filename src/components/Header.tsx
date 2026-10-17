import React from "react";
import { Radar, ShieldAlert, Cpu, GitPullRequest, ExternalLink } from "lucide-react";
import { MockModeStatus } from "../types";

interface HeaderProps {
  mockStatus: MockModeStatus | null;
  activeTab: string;
  onTabChange: (tab: string) => void;
}

export const Header: React.FC<HeaderProps> = ({
  mockStatus,
  activeTab,
  onTabChange,
}) => {
  return (
    <header className="h-14 border-b border-border-subtle bg-surface-panel/95 backdrop-blur px-4 flex items-center justify-between select-none z-20">
      {/* Brand Logo & Navigation */}
      <div className="flex items-center space-x-6">
        <div className="flex items-center space-x-2.5">
          <div className="w-8 h-8 rounded-lg bg-surface-elevated border border-border-strong flex items-center justify-center text-secondary shadow-glow relative overflow-hidden">
            <Radar className="w-5 h-5 text-secondary animate-pulse-subtle" />
          </div>
          <div className="flex items-baseline space-x-1.5">
            <span className="font-headline font-bold text-base tracking-tight text-text-primary">
              CodeRadar
            </span>
            <span className="font-mono text-[10px] font-semibold uppercase px-1.5 py-0.5 rounded bg-surface-elevated text-secondary border border-border-subtle">
              Studio
            </span>
          </div>
        </div>

        {/* Navigation Tabs */}
        <nav className="flex items-center space-x-1 text-xs font-medium">
          <button
            onClick={() => onTabChange("dashboard")}
            className={`px-3 py-1.5 rounded-md transition-colors ${
              activeTab === "dashboard"
                ? "bg-surface-elevated text-secondary border border-border-subtle"
                : "text-text-muted hover:text-text-primary hover:bg-surface-elevated/50"
            }`}
          >
            Dashboard
          </button>
          <button
            onClick={() => onTabChange("architecture")}
            className={`px-3 py-1.5 rounded-md transition-colors flex items-center space-x-1.5 ${
              activeTab === "architecture"
                ? "bg-surface-elevated text-secondary border border-border-subtle"
                : "text-text-muted hover:text-text-primary hover:bg-surface-elevated/50"
            }`}
          >
            <Cpu className="w-3.5 h-3.5 text-[#78da9e]" />
            <span>Architecture</span>
          </button>
          <button
            onClick={() => onTabChange("security")}
            className={`px-3 py-1.5 rounded-md transition-colors flex items-center space-x-1.5 ${
              activeTab === "security"
                ? "bg-surface-elevated text-secondary border border-border-subtle"
                : "text-text-muted hover:text-text-primary hover:bg-surface-elevated/50"
            }`}
          >
            <ShieldAlert className="w-3.5 h-3.5 text-status-critical" />
            <span>Security</span>
          </button>
          <button
            onClick={() => onTabChange("radar")}
            className="px-3 py-1.5 rounded-md transition-colors text-text-muted hover:text-text-primary hover:bg-surface-elevated/50 flex items-center space-x-1.5"
            title="360° Polar Radar Canvas is scheduled for Phase 4"
          >
            <Radar className="w-3.5 h-3.5 text-tertiary" />
            <span>Polar Radar</span>
            <span className="text-[9px] uppercase px-1 py-0.2 rounded bg-[#00B4D8]/10 text-tertiary border border-tertiary/20">
              P4
            </span>
          </button>
          <button
            onClick={() => onTabChange("prs")}
            className="px-3 py-1.5 rounded-md transition-colors text-text-muted/60 hover:text-text-muted flex items-center space-x-1.5"
            title="PR Changeset Diff engine is scheduled for Phase 8"
          >
            <GitPullRequest className="w-3.5 h-3.5" />
            <span>PRs</span>
            <span className="text-[9px] uppercase px-1 py-0.2 rounded bg-surface-elevated text-text-muted border border-border-subtle">
              P8
            </span>
          </button>
        </nav>
      </div>

      {/* Right Telemetry Badges */}
      <div className="flex items-center space-x-3 text-xs">
        {mockStatus && (
          <div
            className="flex items-center space-x-1.5 px-2.5 py-1 rounded-full bg-primary/10 border border-primary/25 text-secondary text-[11px] font-mono"
            title={mockStatus.description}
          >
            <span className="w-1.5 h-1.5 rounded-full bg-secondary animate-ping" />
            <span className="font-semibold">Mock Engine</span>
            <span className="text-text-muted">|</span>
            <span className="text-text-muted">{mockStatus.ruleset_version}</span>
          </div>
        )}

        <a
          href="https://github.com/mahmoudfarahat2647/coderadar"
          target="_blank"
          rel="noreferrer"
          className="p-1.5 text-text-muted hover:text-text-primary hover:bg-surface-elevated rounded-md transition-colors"
          title="View GitHub Repository"
        >
          <ExternalLink className="w-4 h-4" />
        </a>
      </div>
    </header>
  );
};
