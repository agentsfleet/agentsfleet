import { test } from "bun:test";
import assert from "node:assert/strict";
import { extractDistinctIdFromToken } from "../src/program/auth-token.js";

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
