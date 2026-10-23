#!/usr/bin/env node
// Sign a CodeRadar ruleset with an Ed25519 private key (Phase 8, CR-SPEC-04 §7).
//
// The signature covers the canonical JSON of the manifest *without* its
// `signature` field: object keys are sorted recursively and the document is
// serialised compactly. The Rust verifier (`rules_ota.rs`) reproduces exactly
// this payload, so any byte change to a rule invalidates the signature.
//
// Usage:
//   node scripts/sign-rules.mjs <private-key.pem> <manifest.json>

import { createPrivateKey, sign } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";

/** Deterministic JSON: object keys sorted, no whitespace. */
export function canonicalJson(value) {
  if (value === null || typeof value !== "object") {
    return JSON.stringify(value);
  }
  if (Array.isArray(value)) {
    return `[${value.map(canonicalJson).join(",")}]`;
  }
  const keys = Object.keys(value).sort();
  return `{${keys
    .map((key) => `${JSON.stringify(key)}:${canonicalJson(value[key])}`)
    .join(",")}}`;
}

/** The exact bytes the Ed25519 signature is computed over. */
export function signingPayload(manifest) {
  const { signature: _signature, ...unsigned } = manifest;
  return Buffer.from(canonicalJson(unsigned), "utf8");
}

const [keyPathArg, manifestPathArg] = process.argv.slice(2);
if (!keyPathArg || !manifestPathArg) {
  process.stderr.write(
    "usage: node scripts/sign-rules.mjs <private-key.pem> <manifest.json>\n"
  );
  process.exit(2);
}

const keyPath = resolve(keyPathArg);
const manifestPath = resolve(manifestPathArg);
const privateKey = createPrivateKey(readFileSync(keyPath, "utf8"));
if (privateKey.asymmetricKeyType !== "ed25519") {
  process.stderr.write("error: the key is not an Ed25519 private key\n");
  process.exit(1);
}

const text = readFileSync(manifestPath, "utf8");
const manifest = JSON.parse(text);
const signature = sign(null, signingPayload(manifest), privateKey)
  .toString("hex")
  .toLowerCase();
const field = `"signature": "ed25519:${signature}"`;

let signed;
if (/"signature"\s*:\s*"[^"]*"/.test(text)) {
  signed = text.replace(/"signature"\s*:\s*"[^"]*"/, field);
} else {
  const anchor = /("categories"\s*:\s*\[[^\]]*\])/;
  if (!anchor.test(text)) {
    process.stderr.write(
      'error: cannot find the "categories" field to insert the signature after\n'
    );
    process.exit(1);
  }
  signed = text.replace(anchor, `$1,\n  ${field}`);
}

writeFileSync(manifestPath, signed, "utf8");
process.stdout.write(
  `signed ${manifestPath} (ruleset ${manifest.ruleset_version}, ${manifest.total_rules} rules)\n` +
    `signature: ed25519:${signature}\n`
);
