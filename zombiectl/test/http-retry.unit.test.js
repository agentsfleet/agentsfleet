// http-retry.unit.test.js — covers M63_004 §1 apiRequestWithRetry.
// Pins: classifier, backoff caps, jitter bounds, ZOMBIE_NO_RETRY=1
// escape hatch, maxAttempts bounds (1..10), Retry-After floor with
// one-sided positive jitter.

import { test } from "bun:test";
import assert from "node:assert/strict";
import { ApiError, apiRequestWithRetry } from "../src/lib/http.js";

function makeFetch(responses) {
  let i = 0;
  const calls = [];
  const fetchImpl = async (url, init) => {
    calls.push({ url, init });
    const next = responses[Math.min(i, responses.length - 1)];
    i += 1;
    if (typeof next === "function") return next();
    if (next instanceof Error) throw next;
    return next;
  };
  return { fetchImpl, calls: () => calls, count: () => i };
}

function makeResponse({ status = 200, body = {}, headers = {} } = {}) {
  return {
    ok: status >= 200 && status < 300,
    status,
    statusText: String(status),
    headers: { get: (k) => headers[k.toLowerCase()] ?? null },
    text: async () => JSON.stringify(body),
  };
}

const NO_SLEEP = async () => {};
const NO_JITTER = () => 0; // ±20% with random=0 → 0% jitter (deterministic)

test("apiRequestWithRetry: returns body on first 200 with no retry events", async () => {
  const events = [];
  const onAttempt = (info) => events.push({ kind: "attempt", ...info });
  const onRetry = (info) => events.push({ kind: "retry", ...info });
  const { fetchImpl, count } = makeFetch([makeResponse({ body: { ok: true } })]);

  const out = await apiRequestWithRetry("http://x", {
    fetchImpl,
    sleepImpl: NO_SLEEP,
    randomFn: NO_JITTER,
    env: {},
    onAttempt,
    onRetry,
  });

  assert.deepEqual(out, { ok: true });
  assert.equal(count(), 1);
  assert.equal(events.filter((e) => e.kind === "retry").length, 0);
  const terminal = events.find((e) => e.kind === "attempt" && e.terminal);
  assert.equal(terminal.attempt, 1);
  assert.equal(terminal.retryCount, 0);
});

test("apiRequestWithRetry: 503 then 200 fires one retry event + one terminal", async () => {
  const events = [];
  const { fetchImpl, count } = makeFetch([
    makeResponse({ status: 503, body: { error: { code: "HTTP_503", message: "blip" } } }),
    makeResponse({ body: { ok: true } }),
  ]);

  const out = await apiRequestWithRetry("http://x", {
    fetchImpl,
    sleepImpl: NO_SLEEP,
    randomFn: NO_JITTER,
    env: {},
    onAttempt: (i) => events.push({ kind: "attempt", ...i }),
    onRetry: (i) => events.push({ kind: "retry", ...i }),
  });

  assert.deepEqual(out, { ok: true });
  assert.equal(count(), 2);
  const retries = events.filter((e) => e.kind === "retry");
  assert.equal(retries.length, 1);
  assert.equal(retries[0].reason, "5xx");
  const terminal = events.find((e) => e.kind === "attempt" && e.terminal);
  assert.equal(terminal.attempt, 2);
  assert.equal(terminal.retryCount, 1);
});

test("apiRequestWithRetry: three 503s exhausted → ApiError attempt=3 retry_count=2", async () => {
  const events = [];
  const r503 = makeResponse({ status: 503, body: { error: { code: "HTTP_503" } } });
  const { fetchImpl, count } = makeFetch([r503, r503, r503]);

  await assert.rejects(
    () =>
      apiRequestWithRetry("http://x", {
        fetchImpl,
        sleepImpl: NO_SLEEP,
        randomFn: NO_JITTER,
        env: {},
        retry: { maxAttempts: 3 },
        onAttempt: (i) => events.push({ kind: "attempt", ...i }),
        onRetry: (i) => events.push({ kind: "retry", ...i }),
      }),
    (err) => err instanceof ApiError && err.status === 503,
  );

  assert.equal(count(), 3);
  assert.equal(events.filter((e) => e.kind === "retry").length, 2);
  const terminal = events.find((e) => e.kind === "attempt" && e.terminal);
  assert.equal(terminal.attempt, 3);
  assert.equal(terminal.retryCount, 2);
});

test("apiRequestWithRetry: 400 fatal → no retry, one fetch", async () => {
  const events = [];
  const { fetchImpl, count } = makeFetch([
    makeResponse({ status: 400, body: { error: { code: "UZ-VALIDATION-001", message: "bad" } } }),
  ]);

  await assert.rejects(() =>
    apiRequestWithRetry("http://x", {
      fetchImpl,
      sleepImpl: NO_SLEEP,
      randomFn: NO_JITTER,
      env: {},
      onAttempt: (i) => events.push({ kind: "attempt", ...i }),
      onRetry: (i) => events.push({ kind: "retry", ...i }),
    }),
  );

  assert.equal(count(), 1);
  assert.equal(events.filter((e) => e.kind === "retry").length, 0);
});

test("apiRequestWithRetry: TypeError(fetch failed) classified as network and retried", async () => {
  const events = [];
  let n = 0;
  const fetchImpl = async () => {
    n += 1;
    if (n === 1) throw new TypeError("fetch failed");
    return makeResponse({ body: { ok: true } });
  };

  const out = await apiRequestWithRetry("http://x", {
    fetchImpl,
    sleepImpl: NO_SLEEP,
    randomFn: NO_JITTER,
    env: {},
    onAttempt: (i) => events.push({ kind: "attempt", ...i }),
    onRetry: (i) => events.push({ kind: "retry", ...i }),
  });

  assert.deepEqual(out, { ok: true });
  assert.equal(n, 2);
  const retries = events.filter((e) => e.kind === "retry");
  assert.equal(retries.length, 1);
  assert.equal(retries[0].reason, "network");
});

test("apiRequestWithRetry: env escape hatch collapses to single attempt on 503", async () => {
  const events = [];
  const r503 = makeResponse({ status: 503, body: { error: { code: "HTTP_503" } } });
  const { fetchImpl, count } = makeFetch([r503, r503, r503]);

  await assert.rejects(() =>
    apiRequestWithRetry("http://x", {
      fetchImpl,
      sleepImpl: NO_SLEEP,
      randomFn: NO_JITTER,
      env: { ZOMBIE_NO_RETRY: "1" },
      onAttempt: (i) => events.push({ kind: "attempt", ...i }),
      onRetry: (i) => events.push({ kind: "retry", ...i }),
    }),
  );

  assert.equal(count(), 1);
  assert.equal(events.filter((e) => e.kind === "retry").length, 0);
});

test("apiRequestWithRetry: maxAttempts > 10 throws synchronously with CONFIG_INVALID", async () => {
  const { fetchImpl, count } = makeFetch([makeResponse()]);
  await assert.rejects(
    () => apiRequestWithRetry("http://x", { fetchImpl, retry: { maxAttempts: 11 } }),
    (err) => err instanceof ApiError && err.code === "CONFIG_INVALID",
  );
  // No fetch issued before the bound check.
  assert.equal(count(), 0);
});

test("apiRequestWithRetry: maxAttempts < 1 throws synchronously with CONFIG_INVALID", async () => {
  const { fetchImpl, count } = makeFetch([makeResponse()]);
  await assert.rejects(
    () => apiRequestWithRetry("http://x", { fetchImpl, retry: { maxAttempts: 0 } }),
    (err) => err instanceof ApiError && err.code === "CONFIG_INVALID",
  );
  await assert.rejects(
    () => apiRequestWithRetry("http://x", { fetchImpl, retry: { maxAttempts: -1 } }),
    (err) => err instanceof ApiError && err.code === "CONFIG_INVALID",
  );
  assert.equal(count(), 0);
});

test("apiRequestWithRetry: backoff delay falls within ±20% jitter band on 250ms base", async () => {
  // Capture sleep durations to verify jitter math without sleeping.
  const sleeps = [];
  const sleepImpl = async (ms) => { sleeps.push(ms); };
  const { fetchImpl } = makeFetch([
    makeResponse({ status: 503, body: { error: {} } }),
    makeResponse({ body: { ok: true } }),
  ]);

  // randomFn returns 0.5 → jitter factor (0.5*2-1)=0 → exactly base delay.
  await apiRequestWithRetry("http://x", {
    fetchImpl,
    sleepImpl,
    randomFn: () => 0.5,
    env: {},
    retry: { maxAttempts: 3, baseDelayMs: 250, capDelayMs: 2000 },
  });

  assert.equal(sleeps.length, 1);
  // base = 250 * 2^(1-1) = 250; jitter = 250*0.2*0 = 0 → 250
  assert.equal(sleeps[0], 250);
});

test("apiRequestWithRetry: 429 with Retry-After: 5 floors backoff at 5000ms with one-sided positive jitter", async () => {
  const sleeps = [];
  const sleepImpl = async (ms) => { sleeps.push(ms); };
  // randomFn returns 1 → max jitter (+20%) → 5000 + 1000 = 6000
  const r429 = makeResponse({
    status: 429,
    body: { error: { code: "RATE_LIMITED" } },
    headers: { "retry-after": "5" },
  });
  const { fetchImpl } = makeFetch([r429, makeResponse({ body: { ok: true } })]);

  await apiRequestWithRetry("http://x", {
    fetchImpl,
    sleepImpl,
    randomFn: () => 1,
    env: {},
    retry: { maxAttempts: 3, baseDelayMs: 250, capDelayMs: 2000 },
  });

  // Floor is 5000; one-sided positive jitter +0..20% → [5000, 6000].
  // With randomFn()=1, expect 5000 + 5000*0.2*1 = 6000.
  assert.equal(sleeps.length, 1);
  assert.ok(sleeps[0] >= 5000, `delay ${sleeps[0]} must be >= 5000 (server floor)`);
  assert.ok(sleeps[0] <= 6000, `delay ${sleeps[0]} must be <= 6000 (server floor + 20%)`);
});

test("apiRequestWithRetry: backoff caps at capDelayMs even after exponential growth", async () => {
  const sleeps = [];
  const sleepImpl = async (ms) => { sleeps.push(ms); };
  const r503 = makeResponse({ status: 503, body: { error: {} } });
  const { fetchImpl } = makeFetch([r503, r503, r503, r503]);

  await assert.rejects(() =>
    apiRequestWithRetry("http://x", {
      fetchImpl,
      sleepImpl,
      randomFn: () => 0.5, // zero jitter
      env: {},
      retry: { maxAttempts: 4, baseDelayMs: 1000, capDelayMs: 1500 },
    }),
  );

  // Three retries → three sleeps. base*2^(n-1): 1000, 2000, 4000.
  // Capped at 1500. Jitter is zero so we expect exactly 1000, 1500, 1500.
  assert.deepEqual(sleeps, [1000, 1500, 1500]);
});
