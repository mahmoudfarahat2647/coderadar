import React from "react";
import { FindingDTO } from "../types";
import { findingEngineLabel } from "../utils/engineTone";

/** Origin chip with explicitly classified evaluator scores. */
export const EngineChip: React.FC<{ finding: FindingDTO }> = ({ finding }) => (
  <span
    className="inline-flex items-center px-1.5 py-0.5 rounded text-[10px] font-mono text-text-muted bg-surface-panel border border-border-subtle"
    title={`Produced by the ${findingEngineLabel(finding.engine).toLowerCase()} engine`}
  >
    {findingEngineLabel(finding.engine)}
    {finding.model_probability != null && (
      <span className="ml-1 text-tertiary">
        model probability {Math.round(finding.model_probability * 100)}%
      </span>
    )}
    {finding.heuristic_weight != null && (
      <span className="ml-1 text-tertiary">
        heuristic weight {finding.heuristic_weight.toFixed(2)}
      </span>
    )}
  </span>
);
