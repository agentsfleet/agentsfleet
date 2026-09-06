// Where a failure happened decides whether a write is sent again: a socket
// code read from the error's cause, a client-side timeout, or the status the
// server answered with. The dashboard makes the same decisions; the shared
// fixture (http-retry.fixture.test.ts) is the parity proof, this file the
// CLI's own reading of each class.

import { expect, test } from "bun:test";
import { ApiError } from "../src/lib/http.ts";
import { apiRequestWithRetry, type RetryInfo } from "../src/lib/http-retry.ts";
import { asFetchImpl, type ResponseLike } from "./helpers.ts";

const URL = "https://api.example.test/v1/thing";
const NO_SLEEP = async (): Promise<void> => undefined;
const OK: ResponseLike = { ok: true, status: 200, statusText: "OK", headers: { get: () => null }, text: async () => '{"ok":true}' };
const TIMED_OUT = Object.assign(new Error("aborted"), { name: "AbortError" });

function fetchFailed(code: string): TypeError {
  return new TypeError("fetch failed", { cause: Object.assign(new Error(code), { code }) });
}

function failingThenOk(first: Error) {
  let calls = 0;
  const fetchImpl = asFetchImpl(async () => {
    calls += 1;
    if (calls === 1) throw first;
    return OK;
  });
  return { fetchImpl, calls: () => calls };
}

async function run(method: string, first: Error): Promise<{ calls: number; retries: RetryInfo[]; outcome: unknown }> {
  const server = failingThenOk(first);
  const retries: RetryInfo[] = [];
  const outcome = await apiRequestWithRetry(URL, {
    method,
    fetchImpl: server.fetchImpl,
    sleepImpl: NO_SLEEP,
    randomFn: () => 0,
    env: {},
    onRetry: (info) => retries.push(info),
  }).catch((err: unknown) => err);
  return { calls: server.calls(), retries, outcome };
}

test("a write that timed out on the client is sent once; a read is read again", async () => {
  const write = await run("POST", TIMED_OUT);
  expect(write.calls).toBe(1);
  expect(write.retries).toEqual([]);
  expect(write.outcome).toBeInstanceOf(ApiError);
  expect((write.outcome as ApiError).code).toBe("TIMEOUT");

  const read = await run("GET", TIMED_OUT);
  expect(read.calls).toBe(2);
  expect(read.retries.map((r) => r.reason)).toEqual(["timeout"]);
});

test("a write reset after sending is sent once; one refused at connect never left and is sent again", async () => {
  const reset = await run("POST", fetchFailed("ECONNRESET"));
  expect(reset.calls).toBe(1);
  expect(reset.outcome).toBeInstanceOf(TypeError);

  const refused = await run("POST", fetchFailed("ECONNREFUSED"));
  expect(refused.calls).toBe(2);
  expect(refused.retries.map((r) => r.reason)).toEqual(["network"]);
});

test("Bun's shape, the code on the error itself, is read the same way", async () => {
  const refused = await run("POST", Object.assign(new TypeError("Unable to connect"), { code: "ConnectionRefused" }));
  expect(refused.calls).toBe(2);
  const reset = await run("POST", Object.assign(new TypeError("The socket connection was closed"), { code: "ECONNRESET" }));
  expect(reset.calls).toBe(1);
});

test("a Retry-After beyond the cap fails at once, with the answer in hand", async () => {
  const refusal = new ApiError("slow", { status: 429, code: "RATE_LIMITED", retryAfterMs: 120_000 });
  const outcome = await run("GET", refusal);
  expect(outcome.calls).toBe(1);
  expect(outcome.retries).toEqual([]);
  expect(outcome.outcome).toBe(refusal);
});

test("a network failure with no cause is read as sent: a read retries, a write does not", async () => {
  const read = await run("GET", new TypeError("Failed to fetch"));
  expect(read.calls).toBe(2);
  const write = await run("POST", new TypeError("Failed to fetch"));
  expect(write.calls).toBe(1);
});

test("a cause without a string code says nothing about where the failure happened", async () => {
  for (const cause of ["socket hung up", null, { errno: -54 }, { code: 54 }]) {
    const write = await run("POST", new TypeError("fetch failed", { cause }));
    expect(write.calls).toBe(1);
  }
});
