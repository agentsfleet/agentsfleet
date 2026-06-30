#!/usr/bin/env node
// M104: re-mint every integration-test JWT against ONE shared keypair + issuer,
// injecting the `scopes` claim the scope-based auth now requires. Replaces the
// per-file JWKS blobs with a single shared modulus (the shared-fixture module
// `test_scope_tokens.zig` documents it) so the keys can never drift again.
//
// Tokens stay per-file because each carries its own `sub`/`tenant_id` (ownership
// is checked against the seeded workspace's tenant) — only the signing key,
// issuer, audience, and kid are shared.
//
//   node scripts/regen-scope-jwts.mjs           # dry-run
//   node scripts/regen-scope-jwts.mjs --apply   # write files + module
import crypto from "node:crypto";
import { readFileSync, writeFileSync, globSync } from "node:fs";

const APPLY = process.argv.includes("--apply");
const b64u = (b) => Buffer.from(b).toString("base64url");
const dec = (s) => JSON.parse(Buffer.from(s, "base64url").toString("utf8"));

const KID = "test-kid-static";
const ISSUER = "https://clerk.test.agentsfleet.net";
const AUDIENCE = "https://api.agentsfleet.net";
const EXP = 4102444800; // 2100-01-01

// Scope sets (verbatim from auth/scopes.zig DefaultGrant + the catalogue).
const S = {
  tenant: "fleet:admin credential:write apikey:admin fleetkey:write grant:write connector:write billing:read approval:resolve workspace:admin template:write",
  platform: "runner:enroll runner:read runner:write stream:read model:admin platform-key:admin template:admin workspace:any",
  member: "fleet:read fleetkey:read grant:read connector:read billing:read approval:read",
};

// Per-(file-substring, sub) scope overrides for the few role-distinction tests.
// Default: platform_admin → platform; else → tenant.
const OVERRIDES = [];

const scopeFor = (file, p) => {
  for (const o of OVERRIDES) {
    if (file.includes(o.file) && p.sub === o.sub && (!o.role || (p.metadata?.role ?? p.role) === o.role)) return o.scopes;
  }
  if (p.metadata?.platform_admin === true || p.platform_admin === true) return S.platform;
  if (typeof p.scopes === "string") return p.scopes; // already-migrated token: keep its scopes
  return S.tenant;
};

const FILES = globSync("src/agentsfleetd/**/*.zig").filter((f) =>
  /integration_test\.zig$|sse_test_fixtures\.zig$|idor_test\.zig$/.test(f));
const JWT = /eyJ[A-Za-z0-9_-]+\.eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+/g;

// One shared keypair.
const { publicKey, privateKey } = crypto.generateKeyPairSync("rsa", { modulusLength: 2048 });
const jwk = publicKey.export({ format: "jwk" });
const SHARED_JWKS = JSON.stringify({ keys: [{ kty: "RSA", n: jwk.n, e: jwk.e, kid: KID, use: "sig", alg: "RS256" }] });

const tokenMap = new Map(); // old -> new
const oldModuli = new Set();
let mint = 0;
for (const f of FILES) {
  const txt = readFileSync(f, "utf8");
  for (const m of txt.matchAll(/"n"\s*:\s*"([^"]+)"/g)) oldModuli.add(m[1]);
  for (const t of txt.match(JWT) ?? []) {
    if (tokenMap.has(t)) continue;
    const [, p] = t.split(".");
    const payload = dec(p);
    const scopes = scopeFor(f, payload);
    const md = payload.metadata ?? {};
    const np = {
      sub: payload.sub, iss: ISSUER, aud: AUDIENCE, exp: EXP, scopes,
      metadata: { ...(md.tenant_id ? { tenant_id: md.tenant_id } : {}), ...(md.workspace_id ? { workspace_id: md.workspace_id } : {}) },
    };
    const hdr = { alg: "RS256", typ: "JWT", kid: KID };
    const si = `${b64u(JSON.stringify(hdr))}.${b64u(JSON.stringify(np))}`;
    const sig = crypto.createSign("RSA-SHA256").update(si).sign(privateKey);
    tokenMap.set(t, `${si}.${b64u(sig)}`);
    mint++;
    console.log(`  ${f.split("agentsfleetd/")[1]}  sub=${payload.sub}  scopes=[${scopes.split(" ")[0]}…(${scopes.split(" ").length})]`);
  }
}
console.log(`\nMinted ${mint} tokens · shared kid=${KID} iss=${ISSUER} · old moduli to replace: ${oldModuli.size}`);

if (!APPLY) { console.log("DRY RUN — re-run with --apply."); process.exit(0); }

// Write the shared-fixture module.
const MODULE = `//! Shared offline auth fixtures for the integration suite — ONE keypair,
//! issuer, and audience for every DB-backed integration test, so the JWKS can't
//! drift per-file. Tokens themselves stay per-file (each carries its own
//! \`sub\`/\`tenant_id\`); only the verifying key + issuer + audience live here.
//!
//! Regenerate with: node scripts/regen-scope-jwts.mjs --apply

pub const ISSUER = "${ISSUER}";
pub const AUDIENCE = "${AUDIENCE}";
pub const JWKS =
    \\\\${SHARED_JWKS}
;
`;
writeFileSync("src/agentsfleetd/http/test_scope_tokens.zig", MODULE);
console.log("  wrote src/agentsfleetd/http/test_scope_tokens.zig");

// Rewrite each file: replace tokens, old moduli → shared modulus, old issuers → shared.
let edits = 0;
for (const f of FILES) {
  let txt = readFileSync(f, "utf8"); const orig = txt;
  for (const [old, neu] of tokenMap) if (txt.includes(old)) txt = txt.replaceAll(old, neu);
  for (const om of oldModuli) if (om !== jwk.n && txt.includes(om)) txt = txt.replaceAll(om, jwk.n);
  txt = txt.replaceAll("https://clerk.dev.agentsfleet.net", ISSUER);
  // Standardize kid in inline JWKS blobs.
  txt = txt.replace(/("kid":")(rbac-test-kid|m80005-test-kid)(")/g, `$1${KID}$3`);
  if (txt !== orig) { writeFileSync(f, txt); edits++; console.log(`  wrote ${f.split("agentsfleetd/")[1]}`); }
}
console.log(`\nApplied to ${edits} files.`);
