import assert from "node:assert/strict";
import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { StatusBar } from "./StatusBar";
import type { EngineStatus } from "../types";

const healthy = { mode: "jev", is_live: true, degraded: false, badge: "Jev", ruleset_version: "1" } as EngineStatus;
const render = (engineStatus: EngineStatus | null, session: object | null) =>
  renderToStaticMarkup(React.createElement(StatusBar, {
    engineStatus, scanStats: null, isScanning: false, onOpenApiKey: () => {}, session,
  } as React.ComponentProps<typeof StatusBar>));

const mixed = render(healthy, {
  engine_mode: "mock_fallback",
  usage: { primary_batches: 2, fallback_batches: 1, fallback_failures: 0 },
});
assert.match(mixed, /Provider: Jev/);
assert.match(mixed, /Scan: Mock Fallback/);
assert.match(mixed, /Primary batches: 2/);
assert.match(mixed, /Fallback batches: 1/);
assert.match(mixed, /Fallback failures: 0/);

const unknown = render(null, { engine_mode: null, usage: null });
assert.match(unknown, /Provider: Unknown/);
assert.match(unknown, /Scan: Not recorded/);
assert.match(unknown, /Usage not recorded/);
assert.doesNotMatch(render(healthy, null), /Scan:/);

const warning = renderToStaticMarkup(React.createElement(StatusBar, {
  engineStatus: healthy, scanStats: null, session: null, isScanning: false,
  onOpenApiKey: () => {}, historyStatus: "save_failed",
} as React.ComponentProps<typeof StatusBar>));
assert.match(warning, /not saved/i);
assert.match(warning, /export/i);
const temporary = renderToStaticMarkup(React.createElement(StatusBar, {
  engineStatus: healthy, scanStats: null, session: null, isScanning: false,
  onOpenApiKey: () => {}, historyStatus: "in_memory",
} as React.ComponentProps<typeof StatusBar>));
assert.match(temporary, /lost when the app closes/i);
assert.match(temporary, /restart/i);
