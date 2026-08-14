import { test } from "bun:test";
import assert from "node:assert/strict";
import {
  decodeTokenPayload,
  extractDistinctIdFromToken,
} from "../src/program/auth-token.ts";

function makeToken(payload: Record<string, unknown>): string {
  const header = Buffer.from(
    JSON.stringify({ alg: "none", typ: "JWT" }),
  ).toString("base64url");
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
  const missingSub = makeToken({ scope: "admin" });
  const blankSub = makeToken({ sub: "   " });
  assert.equal(extractDistinctIdFromToken(missingSub), null);
  assert.equal(extractDistinctIdFromToken(blankSub), null);
});

test("decodeTokenPayload returns parsed payload object", () => {
  const payload = { sub: "user_1", iat: 1000 };
  const result = decodeTokenPayload(makeToken(payload));
  assert.ok(result, "expected non-null decoded payload");
  assert.equal(result.sub, "user_1");
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
