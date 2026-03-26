import { test } from "bun:test";
import assert from "node:assert/strict";
import { decodeTokenPayload, extractDistinctIdFromToken, extractRoleFromToken } from "../src/program/auth-token.js";

function makeToken(payload) {
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
  assert.equal(extractRoleFromToken(makeToken({ "https://usezombie.dev/role": "ADMIN" })), "admin");
  assert.equal(extractRoleFromToken(makeToken({ role: "owner" })), null);
  assert.equal(extractRoleFromToken("bad-token"), null);
});

test("extractRoleFromToken reads usezombie.com namespace claim", () => {
  assert.equal(extractRoleFromToken(makeToken({ "https://usezombie.com/role": "operator" })), "operator");
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
    extractRoleFromToken(makeToken({ metadata: { "https://usezombie.dev/role": "operator" } })),
    "operator",
  );
  assert.equal(
    extractRoleFromToken(makeToken({ metadata: { "https://usezombie.com/role": "Admin" } })),
    "admin",
  );
});

test("decodeTokenPayload returns parsed payload object", () => {
  const payload = { sub: "user_1", role: "admin", iat: 1000 };
  const result = decodeTokenPayload(makeToken(payload));
  assert.equal(result.sub, "user_1");
  assert.equal(result.role, "admin");
  assert.equal(result.iat, 1000);
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
