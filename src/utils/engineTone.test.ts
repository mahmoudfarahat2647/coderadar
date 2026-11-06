import assert from "node:assert/strict";
import { engineTone, providerDiagnosticLabel } from "./engineTone";
import type { EngineStatus } from "../types";

const untested = { mode: "jev_untested", is_live: false, degraded: false } as EngineStatus;
assert.equal(engineTone(untested).label, "Jev Untested");
assert.match(providerDiagnosticLabel("configured_untested"), /inference.*untested/i);
assert.match(providerDiagnosticLabel("available"), /synthetic/i);
assert.match(providerDiagnosticLabel("unauthorized"), /credential/i);
