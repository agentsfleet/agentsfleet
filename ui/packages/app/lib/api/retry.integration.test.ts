// @vitest-environment node
//
// Integration coverage for lib/api/retry.ts driven end-to-end through the REAL
// transport (global fetch) against a REAL local HTTP server. The unit suite
// (retry.test.ts) stubs global fetch; this proves the retry layer classifies
// genuine HTTP responses, parses a genuine Retry-After header, and replays (or
// refuses to replay) real round-trips. Backoff sleeps are recorded + resolved
// instantly so the network is real but the clock is not.
//
// Runs under the `node` environment (not happy-dom): with `window` undefined the
// client's BASE resolves to API_ORIGIN, which we point at the ephemeral server
// via NEXT_PUBLIC_API_URL. The stub-env + module-reset before the dynamic import
// is what lets the module-load-time origin capture the server we just started —
// and keeps the ApiError class we assert against identical to the one thrown.

import http from "node:http";
import type { AddressInfo } from "node:net";
import { afterAll, beforeEach, describe, expect, it, vi } from "vitest";

import type { RetryInfo } from "./retry";

type Scripted = { status: number; body?: string; headers?: Record<string, string> };

const PATH = "/v1/thing";
// A route the server accepts and never answers — the hung-backend case.
const HANG_PATH = "/v1/hung";
const TOKEN = "test-token";
const OK_BODY = '{"ok":true}';
// Short enough to keep the suite fast, long enough that the request is on the
// wire before it fires.
const HUNG_READ_TIMEOUT_MS = 50;

const queue: Scripted[] = [];
const methodLog: string[] = [];
const hung: http.ServerResponse[] = [];

const server = http.createServer((req, res) => {
  methodLog.push(req.method ?? "");
  if (req.url === HANG_PATH) {
    hung.push(res);
    return;
  }
  const next = queue.shift() ?? { status: 200, body: OK_BODY };
  res.writeHead(next.status, { "content-type": "application/json", ...(next.headers ?? {}) });
  res.end(next.body ?? OK_BODY);
});
await new Promise<void>((resolve, reject) => {
  server.once("error", reject);
  server.listen(0, "127.0.0.1", () => resolve());
});
const { port } = server.address() as AddressInfo;
vi.stubEnv("NEXT_PUBLIC_API_URL", `http://127.0.0.1:${port}`);
// This suite drives the REAL transport against the local server above. Restore
// the real fetch that vitest.setup.ts swaps out for a no-network default in the
// unit suite (it captured the original under `__realFetch`).
globalThis.fetch = (globalThis as { __realFetch?: typeof fetch }).__realFetch ?? globalThis.fetch;
// The unit suite defaults to one attempt (vitest.setup.ts); this suite proves
// the policy over a real socket, so the switch goes back to "retry".
vi.stubEnv("AGENTSFLEET_NO_RETRY", "");
vi.resetModules();
const { request, requestWithRetry } = await import("./client");
const { ApiError } = await import("./errors");
const { RETRY_CODE_TIMEOUT } = await import("./retry");

afterAll(async () => {
  vi.unstubAllEnvs();
  for (const res of hung) res.destroy();
  await new Promise<void>((resolve) => server.close(() => resolve()));
});

beforeEach(() => {
  queue.length = 0;
  methodLog.length = 0;
});

// Real network, fake clock: sleeps are recorded instead of awaited and jitter is
// pinned (randomFn 0.5 → 0 jitter) so backoff math is exact. baseDelayMs is tiny
// purely for readable assertions — the sleep never actually elapses.
function fastRetry(extra: Record<string, number> = {}) {
  const delays: number[] = [];
  const retries: RetryInfo[] = [];
  return {
    delays,
    retries,
    options: {
      baseDelayMs: 10,
      capDelayMs: 2000,
      maxAttempts: 3,
      sleepImpl: async (ms: number) => {
        delays.push(ms);
      },
      randomFn: () => 0.5,
      onRetry: (info: RetryInfo) => retries.push(info),
      ...extra,
    },
  };
}

describe("requestWithRetry — real transport integration", () => {
  it("retries real 503s and returns the eventual 200 body", async () => {
    queue.push({ status: 503 }, { status: 503 }, { status: 200, body: OK_BODY });
    const { options, delays, retries } = fastRetry();
    const body = await requestWithRetry<{ ok: boolean }>(PATH, { method: "GET" }, TOKEN, options);
    expect(body).toEqual({ ok: true });
    expect(methodLog).toEqual(["GET", "GET", "GET"]);
    expect(delays).toEqual([10, 20]); // exponential growth, jitter pinned to 0
    expect(retries.map((r) => [r.reason, r.status])).toEqual([
      ["5xx", 503],
      ["5xx", 503],
    ]);
  });

  it("does not replay a real POST 503 (idempotency gate)", async () => {
    queue.push({ status: 503 });
    const { options } = fastRetry();
    await expect(
      requestWithRetry(PATH, { method: "POST", body: "{}" }, TOKEN, options),
    ).rejects.toMatchObject({ status: 503 });
    expect(methodLog).toEqual(["POST"]); // one round-trip, no replay
  });

  it("replays a real PUT 503 (idempotent method)", async () => {
    queue.push({ status: 503 }, { status: 200, body: OK_BODY });
    const { options } = fastRetry();
    const body = await requestWithRetry<{ ok: boolean }>(
      PATH,
      { method: "PUT", body: "{}" },
      TOKEN,
      options,
    );
    expect(body).toEqual({ ok: true });
    expect(methodLog).toEqual(["PUT", "PUT"]);
  });

  it("honors a real Retry-After header on 429", async () => {
    queue.push({ status: 429, headers: { "Retry-After": "1" } }, { status: 200, body: OK_BODY });
    const { options, delays } = fastRetry();
    const body = await requestWithRetry<{ ok: boolean }>(PATH, { method: "GET" }, TOKEN, options);
    expect(body).toEqual({ ok: true });
    expect(methodLog.length).toBe(2);
    expect(delays[0]).toBeGreaterThanOrEqual(MS_PER_SECOND); // 1s server floor parsed from the header
  });

  it("a hung read times out into the retryable class, and the caller's aborted signal stops the loop", async () => {
    // The caller's own timeout fires on the wire. Its signal stays aborted, so
    // a second attempt could only fail instantly against it — the policy stops
    // instead of sleeping a backoff to prove that. The attempt ceiling for the
    // default (fresh-signal-per-attempt) path is proved in client.defaults.
    const { options, retries } = fastRetry({ maxAttempts: 3 });
    const err = await requestWithRetry(
      HANG_PATH,
      { method: "GET", signal: AbortSignal.timeout(HUNG_READ_TIMEOUT_MS) },
      TOKEN,
      options,
    ).catch((e: unknown) => e);
    expect(err).toBeInstanceOf(ApiError);
    expect((err as InstanceType<typeof ApiError>).code).toBe(RETRY_CODE_TIMEOUT);
    expect(retries).toEqual([]);
    expect(methodLog).toEqual(["GET"]); // the one attempt did reach the server
  });

  it("exhausts maxAttempts on a persistently failing real server", async () => {
    queue.push({ status: 503 }, { status: 503 }, { status: 503 }, { status: 503 });
    const { options } = fastRetry({ maxAttempts: 3 });
    await expect(
      requestWithRetry(PATH, { method: "GET" }, TOKEN, options),
    ).rejects.toBeInstanceOf(ApiError);
    expect(methodLog.length).toBe(3); // capped at maxAttempts, not the 4 queued
  });
});
describe("request — default policy over a real transport", () => {
  it("request retries a transient read and returns the recovered body", async () => {
    queue.push({ status: 503 }, { status: 200, body: OK_BODY });
    // No options at all: the built-in backoff sleeps for real (one base delay).
    const body = await request<{ ok: boolean }>(PATH, { method: "GET" }, TOKEN);
    expect(body).toEqual({ ok: true });
    expect(methodLog).toEqual(["GET", "GET"]);
  });

  it("request does not replay a non-idempotent write on a server error", async () => {
    queue.push({ status: 503 });
    await expect(request(PATH, { method: "POST", body: "{}" }, TOKEN)).rejects.toMatchObject({
      status: 503,
    });
    expect(methodLog).toEqual(["POST"]);
  });
});

const MS_PER_SECOND = 1000 as const;
