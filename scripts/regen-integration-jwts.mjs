#!/usr/bin/env node
// Regenerate the integration-test JWT fixtures whose iss/aud still carried the
// retired `usezombie.com` host (base64-hidden — the M92_004 brand sed could not
// reach inside the encoded JWT payload, so they survived the rename and now fail
// `iss`/`aud` validation against the renamed `agentsfleet.net` expectations).
//
// The signing keypairs are deliberately NOT committed; this regenerates a fresh
// RSA keypair per stale `kid`, re-mints each stale token (renaming only the host
// in iss/aud — every other claim preserved), and rewrites the paired JWKS
// modulus across every integration fixture that carries it.
//
//   node scripts/regen-integration-jwts.mjs           # dry-run (print mapping)
//   node scripts/regen-integration-jwts.mjs --apply   # write files
import crypto from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { globSync } from "node:fs";

const APPLY = process.argv.includes("--apply");
const b64u = (b) => Buffer.from(b).toString("base64url");
const dec = (s) => JSON.parse(Buffer.from(s, "base64url").toString("utf8"));
const FILES = globSync("src/agentsfleetd/**/*.zig").filter((f) =>
  /integration_test\.zig$|sse_test_fixtures\.zig$|cross_workspace_idor_test\.zig$/.test(f));

const JWT = /eyJ[A-Za-z0-9_-]+\.eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+/g;
// 1) collect unique stale tokens (usezombie in iss/aud) + their kid
const stale = new Map(); // token -> {kid, hdr, payload}
for (const f of FILES) {
  for (const t of readFileSync(f, "utf8").match(JWT) ?? []) {
    if (stale.has(t)) continue;
    const [h, p] = t.split(".");
    const hdr = dec(h), payload = dec(p);
    const blob = JSON.stringify(payload);
    if (blob.includes("usezombie")) stale.set(t, { kid: hdr.kid, hdr, payload });
  }
}
// 2) one fresh keypair per stale kid
const kids = [...new Set([...stale.values()].map((v) => v.kid))];
const keyByKid = new Map();
for (const kid of kids) {
  const { publicKey, privateKey } = crypto.generateKeyPairSync("rsa", { modulusLength: 2048 });
  keyByKid.set(kid, { publicKey, privateKey, jwk: publicKey.export({ format: "jwk" }) });
}
// 3) re-mint each stale token (host rename only) + record old->new
const tokenMap = new Map();
for (const [old, { kid, hdr, payload }] of stale) {
  const np = JSON.parse(JSON.stringify(payload));
  for (const k of ["iss", "aud"]) if (typeof np[k] === "string") np[k] = np[k].replaceAll("usezombie.com", "agentsfleet.net");
  const si = `${b64u(JSON.stringify(hdr))}.${b64u(JSON.stringify(np))}`;
  const sig = crypto.createSign("RSA-SHA256").update(si).sign(keyByKid.get(kid).privateKey);
  tokenMap.set(old, { neu: `${si}.${b64u(sig)}`, kid, role: np.metadata?.role, iss: np.iss, aud: np.aud });
}
// 4) old modulus per kid (read from any file carrying that kid)
const oldN = new Map();
for (const f of FILES) {
  const txt = readFileSync(f, "utf8");
  for (const m of txt.matchAll(/"kid"\s*:\s*"([^"]+)"[\s\S]{0,200}?"n"\s*:\s*"([^"]+)"/g)) if (!oldN.has(m[1])) oldN.set(m[1], m[2]);
  for (const m of txt.matchAll(/"n"\s*:\s*"([^"]+)"[\s\S]{0,200}?"kid"\s*:\s*"([^"]+)"/g)) if (!oldN.has(m[2])) oldN.set(m[2], m[1]);
}

console.log(`Stale tokens: ${stale.size}  | stale kids: ${kids.join(", ")}`);
for (const [old, info] of tokenMap) console.log(`  token kid=${info.kid} role=${info.role} iss=${info.iss} aud=${info.aud}\n    ${old.slice(0,28)}… -> ${info.neu.slice(0,28)}…`);
for (const kid of kids) console.log(`  modulus[${kid}]: ${oldN.get(kid)?.slice(0,20)}… -> ${keyByKid.get(kid).jwk.n.slice(0,20)}…`);

if (!APPLY) { console.log("\nDRY RUN — re-run with --apply to write files."); process.exit(0); }
// 5) apply: replace old moduli + old tokens across all files
let edits = 0;
for (const f of FILES) {
  let txt = readFileSync(f, "utf8"), orig = txt;
  for (const kid of kids) { const o = oldN.get(kid), n = keyByKid.get(kid).jwk.n; if (o && txt.includes(o)) txt = txt.replaceAll(o, n); }
  for (const [old, info] of tokenMap) if (txt.includes(old)) txt = txt.replaceAll(old, info.neu);
  if (txt !== orig) { writeFileSync(f, txt); edits++; console.log(`  wrote ${f.split("agentsfleetd/")[1]}`); }
}
console.log(`\nApplied to ${edits} files.`);
