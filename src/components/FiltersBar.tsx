import React from "react";
import { Search, Filter, Shield, Cpu, Zap, Activity, GitPullRequest } from "lucide-react";
import { FilterCategory, FilterSeverity, SeverityTotals } from "../types";

interface FiltersBarProps {
  activeSeverity: FilterSeverity;
  onSeverityChange: (sev: FilterSeverity) => void;
  activeCategory: FilterCategory;
  onCategoryChange: (cat: FilterCategory) => void;
  searchQuery: string;
  onSearchChange: (q: string) => void;
  severityTotals: SeverityTotals;
}

export const FiltersBar: React.FC<FiltersBarProps> = ({
  activeSeverity,
  onSeverityChange,
  activeCategory,
  onCategoryChange,
  searchQuery,
  onSearchChange,
  severityTotals,
}) => {
  return (
    <div className="px-4 py-3 bg-surface-panel border-b border-border-subtle flex flex-col lg:flex-row lg:items-center justify-between gap-3 select-none">
      {/* Left: Severity Filter Pills */}
      <div className="flex flex-wrap items-center gap-1.5 text-xs">
        <span className="text-text-muted text-xs font-medium mr-1 flex items-center space-x-1">
          <Filter className="w-3 h-3 text-text-muted" />
          <span>Severity:</span>
        </span>

        {/* ALL */}
        <button
          onClick={() => onSeverityChange("ALL")}
          className={`px-2.5 py-1 rounded-md font-medium text-xs transition-colors flex items-center space-x-1.5 ${
            activeSeverity === "ALL"
              ? "bg-surface-elevated text-text-primary border border-border-strong font-semibold"
              : "text-text-muted hover:text-text-primary hover:bg-surface-elevated/40"
          }`}
        >
          <span>ALL</span>
          <span className="font-mono text-[10px] px-1.5 py-0.2 rounded-full bg-surface-canvas text-text-muted">
            {severityTotals.total}
          </span>
        </button>

        {/* CRITICAL */}
        <button
          onClick={() => onSeverityChange("CRITICAL")}
          className={`px-2.5 py-1 rounded-md font-medium text-xs transition-colors flex items-center space-x-1.5 ${
            activeSeverity === "CRITICAL"
              ? "bg-status-critical/15 text-status-critical border border-status-critical/40 font-semibold"
              : "text-text-muted hover:text-status-critical hover:bg-status-critical/5"
          }`}
        >
          <span className="w-2 h-2 rounded-full bg-status-critical" />
          <span>CRITICAL</span>
          <span className="font-mono text-[10px] px-1.5 py-0.2 rounded-full bg-surface-canvas text-status-critical">
            {severityTotals.critical}
          </span>
        </button>

        {/* HIGH */}
        <button
          onClick={() => onSeverityChange("HIGH")}
          className={`px-2.5 py-1 rounded-md font-medium text-xs transition-colors flex items-center space-x-1.5 ${
            activeSeverity === "HIGH"
              ? "bg-status-high/15 text-status-high border border-status-high/40 font-semibold"
              : "text-text-muted hover:text-status-high hover:bg-status-high/5"
          }`}
        >
          <span className="w-2 h-2 rounded-full bg-status-high" />
          <span>HIGH</span>
          <span className="font-mono text-[10px] px-1.5 py-0.2 rounded-full bg-surface-canvas text-status-high">
            {severityTotals.high}
          </span>
        </button>

        {/* MEDIUM */}
        <button
          onClick={() => onSeverityChange("MEDIUM")}
          className={`px-2.5 py-1 rounded-md font-medium text-xs transition-colors flex items-center space-x-1.5 ${
            activeSeverity === "MEDIUM"
              ? "bg-status-medium/15 text-status-medium border border-status-medium/40 font-semibold"
              : "text-text-muted hover:text-status-medium hover:bg-status-medium/5"
          }`}
        >
          <span className="w-2 h-2 rounded-full bg-status-medium" />
          <span>MEDIUM</span>
          <span className="font-mono text-[10px] px-1.5 py-0.2 rounded-full bg-surface-canvas text-status-medium">
            {severityTotals.medium}
          </span>
        </button>

        {/* LOW */}
        <button
          onClick={() => onSeverityChange("LOW")}
          className={`px-2.5 py-1 rounded-md font-medium text-xs transition-colors flex items-center space-x-1.5 ${
            activeSeverity === "LOW"
              ? "bg-status-low/15 text-status-low border border-status-low/40 font-semibold"
              : "text-text-muted hover:text-status-low hover:bg-status-low/5"
          }`}
        >
          <span className="w-2 h-2 rounded-full bg-status-low" />
          <span>LOW</span>
          <span className="font-mono text-[10px] px-1.5 py-0.2 rounded-full bg-surface-canvas text-status-low">
            {severityTotals.low}
          </span>
        </button>
      </div>

      {/* Middle: Category Pills */}
      <div className="flex flex-wrap items-center gap-1.5 text-xs">
        <span className="text-text-muted text-xs font-medium mr-1">Pillar:</span>
        <button
          onClick={() => onCategoryChange("ALL")}
          className={`px-2 py-0.5 rounded text-[11px] font-medium transition-colors ${
            activeCategory === "ALL"
              ? "bg-secondary/15 text-secondary border border-secondary/30 font-semibold"
              : "text-text-muted hover:text-text-primary bg-surface-elevated/40"
          }`}
        >
          All
        </button>
        <button
          onClick={() => onCategoryChange("architecture")}
          className={`px-2 py-0.5 rounded text-[11px] font-medium transition-colors flex items-center space-x-1 ${
            activeCategory === "architecture"
              ? "bg-secondary/15 text-secondary border border-secondary/30 font-semibold"
              : "text-text-muted hover:text-text-primary bg-surface-elevated/40"
          }`}
        >
          <Cpu className="w-3 h-3 text-[#78da9e]" />
          <span>Architecture</span>
        </button>
        <button
          onClick={() => onCategoryChange("security")}
          className={`px-2 py-0.5 rounded text-[11px] font-medium transition-colors flex items-center space-x-1 ${
            activeCategory === "security"
              ? "bg-secondary/15 text-secondary border border-secondary/30 font-semibold"
              : "text-text-muted hover:text-text-primary bg-surface-elevated/40"
          }`}
        >
          <Shield className="w-3 h-3 text-status-critical" />
          <span>Security</span>
        </button>
        <button
          onClick={() => onCategoryChange("edge_cases")}
          className={`px-2 py-0.5 rounded text-[11px] font-medium transition-colors flex items-center space-x-1 ${
            activeCategory === "edge_cases"
              ? "bg-secondary/15 text-secondary border border-secondary/30 font-semibold"
              : "text-text-muted hover:text-text-primary bg-surface-elevated/40"
          }`}
        >
          <Zap className="w-3 h-3 text-status-medium" />
          <span>Edge Cases</span>
        </button>
        <button
          onClick={() => onCategoryChange("performance")}
          className={`px-2 py-0.5 rounded text-[11px] font-medium transition-colors flex items-center space-x-1 ${
            activeCategory === "performance"
              ? "bg-secondary/15 text-secondary border border-secondary/30 font-semibold"
              : "text-text-muted hover:text-text-primary bg-surface-elevated/40"
          }`}
        >
          <Activity className="w-3 h-3 text-tertiary" />
          <span>Performance</span>
        </button>
        <button
          onClick={() => onCategoryChange("pull_requests")}
          className={`px-2 py-0.5 rounded text-[11px] font-medium transition-colors flex items-center space-x-1 ${
            activeCategory === "pull_requests"
              ? "bg-secondary/15 text-secondary border border-secondary/30 font-semibold"
              : "text-text-muted hover:text-text-primary bg-surface-elevated/40"
          }`}
        >
          <GitPullRequest className="w-3 h-3 text-text-muted" />
          <span>PRs</span>
        </button>
      </div>

      {/* Right: Search Filter Input */}
      <div className="relative w-full lg:w-60">
        <Search className="w-3.5 h-3.5 text-text-muted absolute left-2.5 top-1/2 -translate-y-1/2" />
        <input
          type="text"
          value={searchQuery}
          onChange={(e) => onSearchChange(e.target.value)}
          placeholder="Filter by file, rule, or CWE..."
          className="w-full bg-surface-canvas border border-border-subtle focus:border-border-strong focus:outline-none rounded-md pl-8 pr-3 py-1 text-xs text-text-primary placeholder:text-text-muted"
        />
      </div>
    </div>
  );
};
