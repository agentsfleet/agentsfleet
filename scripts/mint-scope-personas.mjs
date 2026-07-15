#!/usr/bin/env node
// Persona-based integration-test JWT fixtures (M104).
//
// ONE stable, committed test keypair + a handful of PERSONAS defined once below.
// Every DB-backed integration test aliases `scope_fixtures.<PERSONA>` instead of
// carrying its own copy-pasted token blob. Consequence:
//   • change a persona's scopes  -> edit ONE line here, re-run --apply
//   • review the auth surface     -> read the PERSONAS table (a few lines), not 29 blobs
//   • no per-run churn            -> the keypair is fixed, so re-minting is deterministic
//
//   node scripts/mint-scope-personas.mjs           # dry-run (prints the plan)
//   node scripts/mint-scope-personas.mjs --apply   # write module + rewrite test files
import crypto from "node:crypto";
import { readFileSync, writeFileSync, globSync } from "node:fs";

const APPLY = process.argv.includes("--apply");
const b64u = (b) => Buffer.from(b).toString("base64url");

const KID = "test-kid-static";
const ISSUER = "https://clerk.test.agentsfleet.net";
const AUDIENCE = "https://api.agentsfleet.net";
const EXP = 4102444800; // 2100-01-01 — fixtures never expire under test
const TENANT = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
const WORKSPACE = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";

// Stable, throwaway TEST signing key (RSA-2048, PKCS8 DER, base64). NOT a real
// credential — it only signs offline fixtures verified by the test JWKS below.
// Committed so re-minting is deterministic (no per-run keypair churn).
const PRIV_DER_B64 =
  "MIIEvgIBADANBgkqhkiG9w0BAQEFAASCBKgwggSkAgEAAoIBAQDDagVJXusbXGMdzNcY9wrXEOPWLkVhx4d/xmAG4zhzO+tNDvR5AtWsCsPmk8vRYXIT1YZALxSM66YpQ1y07Yw8hburSdycVfMMNzOQd5OfE8PI8lfnJ4DoZ0RuQabQeQKs8UWp1/r2WnO64SwLB3Wstf8irtY2gtgngDKwXlilx3iNVHDguGx5BngpwwTmClv4S85LkujrP9PZkSKS1VbtgT85UpY6Un72i3U0J8K58fWdXWres3m8CeCKPHPEVGhax2G64d3M4fzEmJhOJ1JMWIswpUzG6cyqtJN0mrCEvIaQFgm8f/HAKYYC0oDT64h477d4FgPBLUWzYuDN6B0PAgMBAAECggEABiwVGqoq1uRmJQzRRnKXHw/jgmJXwqzgtMJ5Sf5nTAx6dCaixie2xAZbBa9pYqWnSsUVxnU2OvU+fFMqXHScV1UKBnEAzNdSI/KiaE++k+m152TwLdbrlWudl7XvJOqNkRTV1HWD8cANo7t0/qwvpVLiSiAnl/UuCqFb+IIiXqoOF5H/+CNdwDvSVuX/ULSMsYICdrXbxwqzkzVaUMrsCTm6JIT4b4PYQFAQsjcN1RWieDMJq0hIMPx2zuMAGD2Psd/IXeWtmHgPZsu/KpK/tPCVexhQK8q3ZagXhIriPDYa6bhaC8bDJoXW/lrol5/MJFtDpOV6l3+ytv4EsKjJsQKBgQDtAKLbwfxru4k/wa7eoxlCq7UV09UCOi9QM3VAMpRXm/hN5uVfLkJJMscsAp/rmlGI9Ee8CAaGeZAFXswgtsOaBKKcmYTQymfaF14XK/EQ/n32R1rKdxa+BpilUF8JKVBeUioZt0d/wZdv+aWXxDuF+CNgUfSMX51MdDk69KznQwKBgQDTE/qbCgotT7jCVbvSU0h8OrFv6D42OYniJSDj15FIBONNDkUHAKafm6h1bucPv3nk48QTd7OxjpOa8NeXqS/bBVO8Q/xnBjrU57Txopfrww73ZL2dQluw2hfT2D/DK+P6wJwTIJNaVoCVpAY1LVIMYLeQQ+ub3fCohwyAKyqYRQKBgQC097ljSBpwQMCqOEBIrA1LxUT+p8OMcdVSzhgHrxdqViQhh9848F+Y+PbwegiWpD0B8FUeFJq27/eywhHoIOX2ovdv0CGENClcdF9aHilyqoCQHygKVSi+bNb91ALdQfimLOMMw9AKk04JKHzzB9nTkAejMrEixpebm1tf0xh7dQKBgQCYBpG+zNJbpEmsHleyuq1AXH2j3h/Aqlx29srjj0ViG7Misp5g1sUru87vFbtyCjTe+HUmmFZiEhCZzdFZuE9xbjrLJCRMh54j7ebTCoplEg5bfMFc3IhxrgLvX5c9GQWQet1uoU3ACQF/xa1663Nm2tobG/A8SPOmTe5g+bYqCQKBgB/vEP8crr7VIhUdFtMhxAI+ylojlH51U8xNLsxom5ih/0VAPeM6phcA4F6ESVaQGrk3tT+sxYB2/5Y7GiIchVSCFwngjLG2QitxSLKXaATIuNB+hJW6t8A+A4Dou7B0fQpB6axjUezxlgUMjZEyCQTO/nIFNTwwYKWokAKsh6/q"; // gitleaks:allow — throwaway RSA test signing key, not a real credential
const JWK_N =
  "w2oFSV7rG1xjHczXGPcK1xDj1i5FYceHf8ZgBuM4czvrTQ70eQLVrArD5pPL0WFyE9WGQC8UjOumKUNctO2MPIW7q0ncnFXzDDczkHeTnxPDyPJX5yeA6GdEbkGm0HkCrPFFqdf69lpzuuEsCwd1rLX_Iq7WNoLYJ4AysF5Ypcd4jVRw4LhseQZ4KcME5gpb-EvOS5Lo6z_T2ZEiktVW7YE_OVKWOlJ-9ot1NCfCufH1nV1q3rN5vAngijxzxFRoWsdhuuHdzOH8xJiYTidSTFiLMKVMxunMqrSTdJqwhLyGkBYJvH_xwCmGAtKA0-uIeO-3eBYDwS1Fs2LgzegdDw";
const JWK_E = "AQAB";

const privateKey = crypto.createPrivateKey({
  key: Buffer.from(PRIV_DER_B64, "base64"),
  format: "der",
  type: "pkcs8",
});

// ── PERSONAS — the single source of truth for test auth identity + scopes. ──
// Scopes are verbatim from auth/scopes.zig (DefaultGrant + catalogue). Change a
// scope string here and re-run --apply; every aliasing test inherits it.
const PERSONAS = {
  // Read-only member. Holds no write/admin/credential scope, so it is the
  // canonical "denied" identity for negative-authz assertions.
  VIEWER: { sub: "user_test", scopes: "fleet:read schedule:read" },
  // Mid-tier: can write fleets + read secrets, but not manage secrets
  // or tenant admin. Exercises the read/write/admin ladder (rbac suite).
  OPERATOR: { sub: "user_test", scopes: "fleet:write schedule:write secret:read" },
  // Full tenant grant — every tenant-plane capability.
  TENANT_ADMIN: {
    sub: "user_test",
    scopes:
      "fleet:admin schedule:write secret:write apikey:admin fleetkey:write grant:write connector:write billing:read approval:resolve workspace:admin library:write",
    },
    // Platform plane (runners, models, platform keys, cross-tenant override).
    PLATFORM_ADMIN: {
      sub: "user_op_m104",
      scopes:
        "runner:enroll runner:read runner:write stream:read model:admin platform-key:admin platform-library:write workspace:any",
  },
  // Full tenant scopes but NO tenant_id/workspace_id claim — proves the null-
  // tenant principal is denied workspace authorization (IDOR fail-closed).
  NO_TENANT: {
    sub: "user_m11_006",
    noTenant: true,
    scopes:
      "fleet:admin schedule:write secret:write apikey:admin fleetkey:write grant:write connector:write billing:read approval:resolve workspace:admin library:write",
  },
};

// ── ALIASES — which persona each test file's local token const resolves to. ──
// Keyed by the path tail under agentsfleetd/. The const keeps its file-local
// name (call sites unchanged); only its definition becomes a persona alias.
const ALIASES = {
  "http/secrets_json_integration_test.zig": { TOKEN_USER: "VIEWER", TOKEN_OPERATOR: "TENANT_ADMIN" },
  "http/fleet_operator_integration_test.zig": { PLATFORM_ADMIN_TOKEN: "PLATFORM_ADMIN", TENANT_ADMIN_TOKEN: "TENANT_ADMIN" },
  "http/fleet_runner_events_integration_test.zig": { PLATFORM_ADMIN_TOKEN: "PLATFORM_ADMIN" },
  "http/rbac_http_integration_test.zig": { TEST_USER_TOKEN: "VIEWER", TEST_OPERATOR_TOKEN: "OPERATOR", TEST_ADMIN_TOKEN: "TENANT_ADMIN" },
  "http/runner_enrollment_integration_test.zig": { OPERATOR_TOKEN: "PLATFORM_ADMIN", TENANT_TOKEN: "TENANT_ADMIN" },
  "http/handlers/cross_workspace_idor_test.zig": { TOKEN_OPERATOR: "TENANT_ADMIN" },
  "http/handlers/tenant_billing_integration_test.zig": { TOKEN_OPERATOR: "TENANT_ADMIN", TOKEN_NO_TENANT: "NO_TENANT" },
  "http/handlers/tenant_workspaces_integration_test.zig": { TOKEN_USER: "TENANT_ADMIN" },
  "http/handlers/workspaces/create_integration_test.zig": { TOKEN_USER: "TENANT_ADMIN" },
  "http/handlers/workspaces/dashboard_integration_test.zig": { TOKEN_USER: "VIEWER", TOKEN_OPERATOR: "TENANT_ADMIN" },
  "http/handlers/memory/memories_integration_test.zig": { TOKEN_OPERATOR: "TENANT_ADMIN" },
  "http/handlers/fleets/api_integration_test.zig": { TOKEN_USER: "TENANT_ADMIN" },
  "http/handlers/fleets/events_integration_test.zig": { TOKEN_OPERATOR: "TENANT_ADMIN" },
  "http/handlers/fleets/messages_integration_test.zig": { TOKEN_OPERATOR: "TENANT_ADMIN" },
  "http/handlers/fleets/patch_body_fields_integration_test.zig": { TOKEN_USER: "TENANT_ADMIN" },
  "http/handlers/fleets/patch_concurrent_integration_test.zig": { TOKEN_OPERATOR: "TENANT_ADMIN" },
  "http/handlers/fleets/sse_test_fixtures.zig": { TOKEN_OPERATOR: "TENANT_ADMIN" },
  "http/handlers/fleet_bundles/api_integration_test.zig": { TOKEN_USER: "TENANT_ADMIN" },
  "http/handlers/approvals/inbox_integration_test.zig": { TOKEN_OPERATOR: "TENANT_ADMIN" },
  "http/handlers/api_keys/tenant_integration_test.zig": { TOKEN_OPERATOR: "TENANT_ADMIN" },
  "http/handlers/admin/model_library_admin_integration_test.zig": { PLATFORM_ADMIN_TOKEN: "PLATFORM_ADMIN", TENANT_ADMIN_TOKEN: "TENANT_ADMIN" },
};

// ── Mint ──
function mint(p) {
  const md = p.noTenant ? {} : { tenant_id: TENANT, workspace_id: WORKSPACE };
  const payload = { sub: p.sub, iss: ISSUER, aud: AUDIENCE, exp: EXP, scopes: p.scopes, metadata: md };
  const hdr = { alg: "RS256", typ: "JWT", kid: KID };
  const si = `${b64u(JSON.stringify(hdr))}.${b64u(JSON.stringify(payload))}`;
  const sig = crypto.createSign("RSA-SHA256").update(si).sign(privateKey);
  return `${si}.${b64u(sig)}`;
}

const tokens = Object.fromEntries(Object.entries(PERSONAS).map(([n, p]) => [n, mint(p)]));
for (const [n, p] of Object.entries(PERSONAS))
  console.log(`  ${n.padEnd(15)} sub=${p.sub.padEnd(14)} scopes=[${p.scopes.split(" ").length}] ${p.noTenant ? "(no tenant claim)" : ""}`);

// ── Verify every aliased file/const exists and maps to a known persona. ──
const SHARED = "src/agentsfleetd/http/test_scope_tokens.zig";
let problems = 0;
const CONST_RE = (name) => new RegExp(`const\\s+${name}\\s*=\\s*\\n?\\s*("eyJ[^"]+"|scope_fixtures\\.[A-Z_]+)\\s*;`);
for (const [tail, map] of Object.entries(ALIASES)) {
  const f = `src/agentsfleetd/${tail}`;
  let txt;
  try { txt = readFileSync(f, "utf8"); } catch { console.log(`  !! missing file ${tail}`); problems++; continue; }
  for (const [cname, persona] of Object.entries(map)) {
    if (!tokens[persona]) { console.log(`  !! ${tail}:${cname} -> unknown persona ${persona}`); problems++; }
    if (!CONST_RE(cname).test(txt)) { console.log(`  !! ${tail}: const ${cname} not found (token-or-alias form)`); problems++; }
  }
}
console.log(`\n${Object.keys(PERSONAS).length} personas · ${Object.keys(ALIASES).length} files · ${Object.values(ALIASES).reduce((a, m) => a + Object.keys(m).length, 0)} aliased consts · problems=${problems}`);

if (!APPLY) { console.log("DRY RUN — re-run with --apply to write."); process.exit(problems ? 1 : 0); }
if (problems) { console.log("Refusing to apply with problems above."); process.exit(1); }

// ── Write the shared persona module. ──
const SHARED_JWKS = JSON.stringify({ keys: [{ kty: "RSA", n: JWK_N, e: JWK_E, kid: KID, use: "sig", alg: "RS256" }] });
const personaDoc = Object.entries(PERSONAS)
  .map(([n, p]) => `//!   ${n.padEnd(15)} ${p.scopes}`)
  .join("\n");
const personaConsts = Object.entries(tokens)
  .map(([n, t]) => `/// Persona — see scopes in the module header. Minted by scripts/mint-scope-personas.mjs.\npub const ${n} =\n    "${t}";`)
  .join("\n");
const MODULE = `//! Shared offline auth fixtures for the integration suite — ONE committed
//! keypair + a fixed set of PERSONAS, so the JWKS can't drift per-file and a
//! scope change is a one-line edit in scripts/mint-scope-personas.mjs.
//!
//! Personas (scope sets):
${personaDoc}
//!
//! Regenerate with: node scripts/mint-scope-personas.mjs --apply

pub const ISSUER = "${ISSUER}";
pub const AUDIENCE = "${AUDIENCE}";
pub const JWKS =
    \\\\${SHARED_JWKS}
;

${personaConsts}
`;
writeFileSync(SHARED, MODULE);
console.log(`  wrote ${SHARED.split("agentsfleetd/")[1]}`);

// ── Rewrite each test file: const <NAME> = <blob|alias>; -> persona alias. ──
let edits = 0;
for (const [tail, map] of Object.entries(ALIASES)) {
  const f = `src/agentsfleetd/${tail}`;
  let txt = readFileSync(f, "utf8");
  const orig = txt;
  for (const [cname, persona] of Object.entries(map)) {
    txt = txt.replace(CONST_RE(cname), `const ${cname} = scope_fixtures.${persona};`);
  }
  if (txt !== orig) { writeFileSync(f, txt); edits++; console.log(`  wrote ${tail}`); }
}
console.log(`\nApplied to ${edits} files.`);
