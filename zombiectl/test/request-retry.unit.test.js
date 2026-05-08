// request-retry.unit.test.js — pins the §3 propagation contract:
// request(ctx, path) reads ctx.retryConfig and forwards it to
// apiRequestWithRetry. Recovery from a transient 503 is therefore
// transparent to handlers that don't opt out via runCommand({ retry: false }).

import { test } from "bun:test";
import assert from "node:assert/strict";
import { request } from "../src/program/http-client.js";

function makeFetchSeq(responses) {
  let i = 0;
  let calls = 0;
  return {
    fetch: async (url) => {
      calls += 1;
      const r = responses[Math.min(i, responses.length - 1)];
      i += 1;
      if (typeof r === "function") return r(url);
      if (r instanceof Error) throw r;
      return r;
    },
    callCount: () => calls,
  };
}

function makeResponse({ status = 200, body = {} } = {}) {
  return {
    ok: status >= 200 && status < 300,
    status,
    statusText: String(status),
    headers: { get: () => null },
    text: async () => JSON.stringify(body),
  };
}

test("request: ctx.retryConfig undefined → default 3 attempts (recovers from one 503)", async () => {
  const seq = makeFetchSeq([
    makeResponse({ status: 503, body: { error: { code: "HTTP_503" } } }),
    makeResponse({ body: { ok: true } }),
  ]);
  const ctx = {
    apiUrl: "http://api.test",
    fetchImpl: seq.fetch,
    // retryConfig undefined → defer to apiRequestWithRetry default.
  };
  // Disable real backoff for the test by passing retry overrides through
  // options (apiRequestWithRetry supports per-call overrides).
  const out = await request(ctx, "/v1/ping", {
    sleepImpl: async () => {},
    randomFn: () => 0.5,
  });
  assert.deepEqual(out, { ok: true });
  assert.equal(seq.callCount(), 2);
});

test("request: ctx.retryConfig={maxAttempts:1} from runCommand({retry:false}) collapses to single attempt", async () => {
  const seq = makeFetchSeq([
    makeResponse({ status: 503, body: { error: { code: "HTTP_503" } } }),
    makeResponse({ body: { ok: true } }), // never reached
  ]);
  const ctx = {
    apiUrl: "http://api.test",
    fetchImpl: seq.fetch,
    retryConfig: { maxAttempts: 1 },
  };
  await assert.rejects(() =>
    request(ctx, "/v1/ping", {
      sleepImpl: async () => {},
      randomFn: () => 0.5,
    }),
  );
  assert.equal(seq.callCount(), 1);
});

test("request: ctx.retryConfig={maxAttempts:5} propagates verbatim", async () => {
  const r503 = makeResponse({ status: 503, body: { error: { code: "HTTP_503" } } });
  const seq = makeFetchSeq([r503, r503, r503, r503, r503]);
  const ctx = {
    apiUrl: "http://api.test",
    fetchImpl: seq.fetch,
    retryConfig: { maxAttempts: 5 },
  };
  await assert.rejects(() =>
    request(ctx, "/v1/ping", {
      sleepImpl: async () => {},
      randomFn: () => 0.5,
    }),
  );
  assert.equal(seq.callCount(), 5);
});

test("request: emits cli_http_request + cli_http_retry through analyticsClient", async () => {
  const events = [];
  const analyticsClient = { capture: (e) => events.push(e) };
  const seq = makeFetchSeq([
    makeResponse({ status: 503, body: { error: { code: "HTTP_503" } } }),
    makeResponse({ body: { ok: true } }),
  ]);
  const ctx = {
    apiUrl: "http://api.test",
    fetchImpl: seq.fetch,
    analyticsClient,
    distinctId: "u-test",
  };
  await request(ctx, "/v1/ping", {
    sleepImpl: async () => {},
    randomFn: () => 0.5,
  });
  const names = events.map((e) => e.event);
  assert.ok(names.includes("cli_http_retry"), "cli_http_retry should fire");
  assert.ok(names.includes("cli_http_request"), "cli_http_request (terminal) should fire");
  const terminal = events.find((e) => e.event === "cli_http_request");
  assert.equal(terminal.properties.attempt, "2");
  assert.equal(terminal.properties.retry_count, "1");
  assert.equal(terminal.distinctId, "u-test");
});
