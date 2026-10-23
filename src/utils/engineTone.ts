import { EngineStatus, FindingEngine } from "../types";

export interface EngineTone {
  /** Badge wrapper classes. */
  wrapper: string;
  /** Status dot class. */
  dot: string;
  /** Short engine label. */
  label: string;
}

/** Visual tone + label for the current engine mode, shared by Header and StatusBar. */
export function engineTone(status: EngineStatus | null): EngineTone {
  if (status?.is_live) {
    return {
      wrapper: "bg-status-low/10 border-status-low/30 text-status-low",
      dot: "bg-status-low animate-ping",
      label: "Jev Tier 1",
    };
  }
  if (status?.degraded) {
    return {
      wrapper: "bg-status-high/10 border-status-high/30 text-status-high",
      dot: "bg-status-high",
      label: "Mock Fallback",
    };
  }
  return {
    wrapper: "bg-primary/10 border-primary/25 text-secondary",
    dot: "bg-secondary animate-ping",
    label: "Mock Engine",
  };
}

/** Label for the engine mode recorded on a scan session or history row. */
export function engineModeLabel(mode: string | null | undefined): string {
  switch (mode) {
    case "jev":
      return "Jev Tier 1";
    case "mock_fallback":
      return "Mock Fallback";
    case "mock":
      return "Mock Engine";
    default:
      return "Not recorded";
  }
}

/** Label of the engine that produced a single finding. */
export function findingEngineLabel(engine: FindingEngine): string {
  switch (engine) {
    case "tier0":
      return "Static";
    case "tier1":
      return "Live model";
    default:
      return "Heuristic";
  }
}
