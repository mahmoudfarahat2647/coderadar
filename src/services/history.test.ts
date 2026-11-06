import assert from "node:assert/strict";
import { getScanHistory } from "./tauri";

async function main() {
  assert.deepEqual(await getScanHistory(), []);
  Object.assign(globalThis, { window: { __TAURI_INTERNALS__: {
    invoke: async () => { throw "database unavailable"; },
  } } });
  await assert.rejects(getScanHistory(), (error) => error === "database unavailable");
}

main();
