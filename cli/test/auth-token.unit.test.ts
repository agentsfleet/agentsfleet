import { test } from "bun:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { decodeTokenPayload, extractDistinctIdFromToken, extractRoleFromToken } from "../src/program/auth-token.ts";

const AUTH_TOKEN_SOURCE = readFileSync(join(import.meta.dir, "..", "src", "program", "auth-token.ts"), "utf8");

function makeToken(payload: Record<string, unknown>): string {
  const header = Buffer.from(JSON.stringify({ alg: "none", typ: "JWT" })).toString("base64url");
  const body = Buffer.from(JSON.stringify(payload)).toString("base64url");
  return `${header}.${body}.sig`;
}

test("extractDistinctIdFromToken returns sub for valid JWT payload", () => {
  const token = makeToken({ sub: "user_123" });
  assert.equal(extractDistinctIdFromToken(token), "user_123");
});

test("extractDistinctIdFromToken trims and returns normalized sub", () => {
  const token = makeToken({ sub: "  user_trim  " });
  assert.equal(extractDistinctIdFromToken(token), "user_trim");
});

test("extractDistinctIdFromToken returns null for malformed token formats", () => {
  assert.equal(extractDistinctIdFromToken("bad-token"), null);
  assert.equal(extractDistinctIdFromToken("a.b"), null);
  assert.equal(extractDistinctIdFromToken(""), null);
  assert.equal(extractDistinctIdFromToken(null), null);
});

test("extractDistinctIdFromToken returns null when sub is missing or blank", () => {
  const missingSub = makeToken({ role: "admin" });
  const blankSub = makeToken({ sub: "   " });
  assert.equal(extractDistinctIdFromToken(missingSub), null);
  assert.equal(extractDistinctIdFromToken(blankSub), null);
});

test("extractRoleFromToken reads supported role claims", () => {
  assert.equal(extractRoleFromToken(makeToken({ role: "admin" })), "admin");
  assert.equal(extractRoleFromToken(makeToken({ metadata: { role: "operator" } })), "operator");
  assert.equal(extractRoleFromToken(makeToken({ custom_claims: { role: "user" } })), "user");
});

test("extractRoleFromToken normalizes namespaced and invalid claims", () => {
  assert.equal(extractRoleFromToken(makeToken({ "https://agentsfleet.net/role": "ADMIN" })), "admin");
  assert.equal(extractRoleFromToken(makeToken({ role: "owner" })), null);
  assert.equal(extractRoleFromToken("bad-token"), null);
});

test("extractRoleFromToken reads agentsfleet.net namespace claim", () => {
  assert.equal(extractRoleFromToken(makeToken({ "https://agentsfleet.net/role": "operator" })), "operator");
});

test("extractRoleFromToken returns first valid role in priority order", () => {
  // top-level role wins over metadata.role
  assert.equal(extractRoleFromToken(makeToken({ role: "admin", metadata: { role: "user" } })), "admin");
  // metadata.role wins when top-level is absent
  assert.equal(extractRoleFromToken(makeToken({ metadata: { role: "user" }, custom_claims: { role: "admin" } })), "user");
});

test("extractRoleFromToken returns null for empty or whitespace-only role", () => {
  assert.equal(extractRoleFromToken(makeToken({ role: "" })), null);
  assert.equal(extractRoleFromToken(makeToken({ role: "   " })), null);
});

test("extractRoleFromToken rejects whitespace-padded roles (matches backend parseAuthRole)", () => {
  // Backend rbac.parseAuthRole rejects " operator " — CLI must match.
  assert.equal(extractRoleFromToken(makeToken({ role: " operator " })), null);
  assert.equal(extractRoleFromToken(makeToken({ role: " admin" })), null);
  assert.equal(extractRoleFromToken(makeToken({ role: "user " })), null);
});

test("extractRoleFromToken returns null for null/undefined token", () => {
  assert.equal(extractRoleFromToken(null), null);
  assert.equal(extractRoleFromToken(undefined), null);
});

test("extractRoleFromToken reads app_metadata.role", () => {
  assert.equal(extractRoleFromToken(makeToken({ app_metadata: { role: "operator" } })), "operator");
  assert.equal(extractRoleFromToken(makeToken({ app_metadata: { role: "Admin" } })), "admin");
});

test("extractRoleFromToken reads namespaced metadata claims", () => {
  assert.equal(
    extractRoleFromToken(makeToken({ metadata: { "https://agentsfleet.net/role": "operator" } })),
    "operator",
  );
  assert.equal(
    extractRoleFromToken(makeToken({ metadata: { "https://agentsfleet.net/role": "Admin" } })),
    "admin",
  );
});

// Standing guard: the decoder once declared ROLE_NAMESPACE_DEV and
// ROLE_NAMESPACE_COM as byte-identical strings and probed both, so half the
// candidate list was a no-op. Role resolution never changes if a duplicate
// creeps back, so only source inspection can catch the regression.
test("auth-token source declares exactly one role-namespace constant", () => {
  const declared = [...AUTH_TOKEN_SOURCE.matchAll(/const\s+(\w*ROLE_NAMESPACE\w*)\s*=/g)].map((m) => m[1]);
  assert.deepEqual(declared, ["ROLE_NAMESPACE"]);
  assert.equal(/ROLE_NAMESPACE_(?:DEV|COM)/.test(AUTH_TOKEN_SOURCE), false);
});

test("decodeTokenPayload returns parsed payload object", () => {
  const payload = { sub: "user_1", role: "admin", iat: 1000 };
  const result = decodeTokenPayload(makeToken(payload));
  assert.ok(result, "expected non-null decoded payload");
  assert.equal(result.sub, "user_1");
  assert.equal(result.role, "admin");
  assert.equal(result.iat, TEST_TOKEN_COUNT);
});

test("decodeTokenPayload returns null for non-string input", () => {
  assert.equal(decodeTokenPayload(null), null);
  assert.equal(decodeTokenPayload(undefined), null);
  assert.equal(decodeTokenPayload(42), null);
  assert.equal(decodeTokenPayload(""), null);
});

test("decodeTokenPayload returns null for malformed base64", () => {
  assert.equal(decodeTokenPayload("header.!!!.sig"), null);
});

test("decodeTokenPayload returns null for token with fewer than 2 parts", () => {
  assert.equal(decodeTokenPayload("single-segment"), null);
});
const TEST_TOKEN_COUNT = 1000 as const;
