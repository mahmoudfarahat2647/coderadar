import { EngineStatus, FindingEngine, ProviderState } from "../types";

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
  if (!status) {
    return {
      wrapper: "bg-primary/10 border-primary/25 text-text-muted",
      dot: "bg-text-muted",
      label: "Unknown",
    };
  }
  if (status?.mode === "jev_untested") {
    return {
      wrapper: "bg-primary/10 border-primary/25 text-secondary",
      dot: "bg-secondary",
      label: "Jev Untested",
    };
  }
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

export function providerDiagnosticLabel(state: ProviderState): string {
  switch (state) {
    case "not_configured": return "No credential configured.";
    case "configured_untested": return "Authentication succeeded; inference is untested.";
    case "available": return "Synthetic inference succeeded.";
    case "unauthorized": return "Credential rejected.";
    case "forbidden": return "Credential lacks access.";
    case "quota_blocked": return "Payment or quota blocked.";
    case "rate_limited": return "Provider rate limit reached.";
    case "unreachable": return "Provider unreachable.";
    case "protocol_incompatible": return "Provider response incompatible.";
    default: return "Unknown provider status.";
  }
}

/** Label for the engine mode recorded on a scan session or history row. */
export function engineModeLabel(mode: string | null | undefined): string {
  switch (mode) {
    case "jev":
      return "Jev Tier 1";
    case "jev_untested":
      return "Jev Untested";
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
    case "mock":
      return "Heuristic";
    default:
      return "Unknown";
  }
}
