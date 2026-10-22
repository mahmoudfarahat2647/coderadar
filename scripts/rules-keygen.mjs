#!/usr/bin/env node
// Generate the Ed25519 key pair used to sign CodeRadar rulesets (Phase 8).
//
// The private key NEVER belongs in the repository: write it outside the
// working tree (e.g. a password manager vault or CI secret store) and pass its
// path to `scripts/sign-rules.mjs`. The printed public key is the only half
// that is embedded in the binary (`rules_ota.rs`).

import { generateKeyPairSync } from "node:crypto";
import { chmodSync, mkdirSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";

const args = process.argv.slice(2);
const outIndex = args.indexOf("--out");
const outPath =
  outIndex >= 0 && args[outIndex + 1]
    ? resolve(args[outIndex + 1])
    : resolve(process.cwd(), "coderadar-rules-signing-key.pem");

const { publicKey, privateKey } = generateKeyPairSync("ed25519");
const privatePem = privateKey.export({ format: "pem", type: "pkcs8" });
const publicRaw = publicKey.export({ format: "der", type: "spki" }).subarray(-32);

mkdirSync(dirname(outPath), { recursive: true });
writeFileSync(outPath, privatePem, { mode: 0o600 });
try {
  chmodSync(outPath, 0o600);
} catch {
  // Windows ACLs do not use POSIX mode bits; keep the file user-private by
  // writing it outside the repository instead.
}

process.stdout.write(
  [
    `private key: ${outPath}`,
    `public key (hex): ${publicRaw.toString("hex")}`,
    `public key (base64): ${publicRaw.toString("base64")}`,
    "",
  ].join("\n")
);
