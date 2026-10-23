import React from "react";
import { FindingDTO } from "../types";
import { findingEngineLabel } from "../utils/engineTone";

/** Origin chip: which engine produced the finding and with what confidence. */
export const EngineChip: React.FC<{ finding: FindingDTO }> = ({ finding }) => (
  <span
    className="inline-flex items-center px-1.5 py-0.5 rounded text-[10px] font-mono text-text-muted bg-surface-panel border border-border-subtle"
    title={`Produced by the ${findingEngineLabel(finding.engine).toLowerCase()} engine`}
  >
    {findingEngineLabel(finding.engine)}
    {finding.confidence < 1 && (
      <span className="ml-1 text-tertiary">
        {Math.round(finding.confidence * 100)}%
      </span>
    )}
  </span>
);
