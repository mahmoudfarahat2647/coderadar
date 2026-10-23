import React from "react";
import { Folder, Play, RefreshCw, GitCommit, FileSearch, Sparkles } from "lucide-react";
import { ScanMode } from "../types";

interface ScanBarProps {
  targetPath: string;
  onPathChange: (path: string) => void;
  onPickFolder: () => void;
  scanMode: ScanMode;
  onScanModeChange: (mode: ScanMode) => void;
  onStartScan: () => void;
  isScanning: boolean;
  onUseFixture: () => void;
}

export const ScanBar: React.FC<ScanBarProps> = ({
  targetPath,
  onPathChange,
  onPickFolder,
  scanMode,
  onScanModeChange,
  onStartScan,
  isScanning,
  onUseFixture,
}) => {
  return (
    <div className="p-4 bg-surface-panel/80 border-b border-border-subtle flex flex-col md:flex-row md:items-center justify-between gap-3 select-none">
      {/* Path Input & Native Picker Button */}
      <div className="flex-1 flex items-center space-x-2">
        <button
          onClick={onPickFolder}
          disabled={isScanning}
          className="px-3 py-2 rounded-lg bg-surface-elevated hover:bg-surface-elevated/80 border border-border-subtle hover:border-border-strong text-text-primary text-xs font-medium flex items-center space-x-2 transition-colors disabled:opacity-50 shrink-0"
          title="Open Native Folder Picker Dialog"
        >
          <Folder className="w-4 h-4 text-tertiary" />
          <span>Pick Folder</span>
        </button>

        <div className="relative flex-1">
          <input
            type="text"
            value={targetPath}
            onChange={(e) => onPathChange(e.target.value)}
            disabled={isScanning}
            placeholder="Select or enter directory path to scan..."
            className="w-full bg-surface-canvas border border-border-subtle focus:border-secondary focus:outline-none rounded-lg px-3 py-2 text-xs font-mono text-text-primary placeholder:text-text-muted transition-colors"
          />
        </div>

        <button
          onClick={onUseFixture}
          disabled={isScanning}
          className="px-2.5 py-2 rounded-lg bg-surface-elevated/60 hover:bg-surface-elevated border border-border-subtle text-text-muted hover:text-text-primary text-xs font-mono flex items-center space-x-1.5 transition-colors shrink-0"
          title="Load curated Known-Bad test fixture"
        >
          <Sparkles className="w-3.5 h-3.5 text-secondary" />
          <span className="hidden sm:inline">Known-Bad Fixture</span>
        </button>
      </div>

      {/* Mode Toggle & Scan Trigger */}
      <div className="flex items-center space-x-3 shrink-0">
        {/* Full Scan vs Git Staged Diff Toggle */}
        <div className="bg-surface-canvas border border-border-subtle rounded-lg p-0.5 flex items-center text-xs">
          <button
            onClick={() => onScanModeChange("full_scan")}
            disabled={isScanning}
            className={`px-3 py-1.5 rounded-md font-medium transition-colors flex items-center space-x-1.5 ${
              scanMode === "full_scan"
                ? "bg-surface-elevated text-secondary font-semibold border border-border-subtle"
                : "text-text-muted hover:text-text-primary"
            }`}
          >
            <FileSearch className="w-3.5 h-3.5" />
            <span>Full Project</span>
          </button>
          <button
            onClick={() => onScanModeChange("staged_diff")}
            disabled={isScanning}
            className={`px-3 py-1.5 rounded-md font-medium transition-colors flex items-center space-x-1.5 ${
              scanMode === "staged_diff"
                ? "bg-surface-elevated text-secondary font-semibold border border-border-subtle"
                : "text-text-muted hover:text-text-primary"
            }`}
            title="Scan only the Git staged changes and score the pull request hygiene"
          >
            <GitCommit className="w-3.5 h-3.5" />
            <span>Git Staged Diff</span>
          </button>
        </div>

        {/* Start Scan Button */}
        <button
          onClick={onStartScan}
          disabled={isScanning || !targetPath.trim()}
          className="px-4 py-2 rounded-lg bg-primary hover:bg-[#239f63] active:bg-[#186f44] text-white text-xs font-semibold flex items-center space-x-2 transition-all shadow-glow disabled:opacity-50 disabled:shadow-none"
        >
          {isScanning ? (
            <>
              <RefreshCw className="w-4 h-4 animate-spin text-white" />
              <span>Scanning...</span>
            </>
          ) : (
            <>
              <Play className="w-4 h-4 fill-current" />
              <span>Run Radar Scan</span>
            </>
          )}
        </button>
      </div>
    </div>
  );
};
