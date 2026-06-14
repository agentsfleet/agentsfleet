// Integration coverage for src/lib/http-retry.ts driven end-to-end through the
// REAL transport (global fetch) against a REAL local HTTP server. The unit
// suite (http-retry.unit.test.ts) mocks fetchImpl; this proves the retry layer
// classifies genuine HTTP responses, parses a genuine Retry-After header, and
// replays (or refuses to replay) real round-trips. Backoff sleeps are recorded
// + resolved instantly so the network is real but the clock is not.

import { afterEach, expect, test } from "bun:test";

import { apiRequestWithRetry } from "../src/lib/http-retry.ts";
import { ApiError } from "../src/lib/http.ts";

type ScriptedResponse = { status: number; body?: string; headers?: Record<string, string> };
const OK_BODY = '{"ok":true}';

let running: { stop: () => void } | null = null;

afterEach(() => {
  running?.stop();
  running = null;
});

function makeServer(script: ScriptedResponse[]) {
  const requests: string[] = [];
  const queue = [...script];
  const server = Bun.serve({
    port: 0,
    fetch(req) {
      requests.push(req.method);
      const next = queue.shift() ?? { status: 200, body: OK_BODY };
      return new Response(next.body ?? OK_BODY, { status: next.status, headers: next.headers ?? {} });
    },
  });
  const handle = { url: `http://localhost:${server.port}/v1/thing`, requests, stop: () => server.stop(true) };
  running = handle;
  return handle;
}

function fastRetry(extra: Record<string, number> = {}) {
  const delays: number[] = [];
  return {
    delays,
    options: {
      env: {} as NodeJS.ProcessEnv,
      sleepImpl: async (ms: number) => {
        delays.push(ms);
      },
      randomFn: () => 0.5,
      retry: { baseDelayMs: 10, capDelayMs: 2000, maxAttempts: 3, ...extra },
    },
  };
}

test("integration: retries real 503s and returns the eventual 200 body", async () => {
  const srv = makeServer([{ status: 503 }, { status: 503 }, { status: 200, body: OK_BODY }]);
  const { options, delays } = fastRetry();
  const body = await apiRequestWithRetry(srv.url, options);
  expect(body).toEqual({ ok: true });
  expect(srv.requests).toEqual(["GET", "GET", "GET"]);
  expect(delays.length).toBe(2);
  expect(delays.every((d) => d > 0)).toBe(true);
  expect(delays[1] ?? 0).toBeGreaterThan(delays[0] ?? 0); // exponential growth
});

test("integration: a real POST 503 is not replayed (idempotency gate)", async () => {
  const srv = makeServer([{ status: 503 }]);
  const { options } = fastRetry();
  await expect(
    apiRequestWithRetry(srv.url, { method: "POST", body: "{}", ...options }),
  ).rejects.toMatchObject({ status: 503 });
  expect(srv.requests).toEqual(["POST"]); // exactly one round-trip, no replay
});

test("integration: honors a real Retry-After header on 429", async () => {
  const srv = makeServer([
    { status: 429, headers: { "Retry-After": "1" } },
    { status: 200, body: OK_BODY },
  ]);
  const { options, delays } = fastRetry();
  const body = await apiRequestWithRetry(srv.url, options);
  expect(body).toEqual({ ok: true });
  expect(srv.requests.length).toBe(2);
  expect(delays[0]).toBeGreaterThanOrEqual(MS_PER_SECOND); // 1s server floor parsed from the header
});

test("integration: exhausts maxAttempts on a persistently failing real server", async () => {
  const srv = makeServer([{ status: 503 }, { status: 503 }, { status: 503 }, { status: 503 }]);
  const { options } = fastRetry({ maxAttempts: 3 });
  await expect(apiRequestWithRetry(srv.url, options)).rejects.toBeInstanceOf(ApiError);
  expect(srv.requests.length).toBe(3); // capped at maxAttempts, not the 4 queued
});
const MS_PER_SECOND = 1000 as const;
