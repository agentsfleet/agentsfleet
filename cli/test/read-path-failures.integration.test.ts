// API failure modes on the AUTHED read path (`agentsfleet list`).
//
// Scope vs the sibling suites — these are deliberately the EDGES the other
// two files do not cover, exercised end-to-end through the real CLI entry
// point (runCli) with on-disk credentials seeded, so the assertions pin the
// *operator-visible* contract (exit code + stderr text + bounded round-trips)
// rather than the library internals:
//
//   - failure-modes.integration.test.ts pins login/workspace/install/logs/
//     doctor UZ surfaces. It never drives `list`, never asserts a
//     bounded call-count from the MockCall ledger, and never covers a
//     request timeout or an ECONNREFUSED-vs-auth message disambiguation.
//   - http-retry.{unit,integration}.test.ts pin apiRequestWithRetry in
//     isolation (classifier, jitter math, Retry-After floor, idempotency
//     gate). They call the retry function directly — NOT through runCli, NOT
//     with seeded creds, and NOT through the Effect ServerError/NetworkError
//     exit-code mapping. This file proves the wired read path inherits the
//     same boundedness and renders the right exit codes.
//
// Real UZ codes (src/agentsfleetd/errors/error_entries.zig) used here:
//   UZ-INTERNAL-002  (500, "Database error")                       :53
//   UZ-API-002       (503, "Event-stream capacity reached")        :85
//   UZ-API-001       (429, "Too many in-flight requests")          :83
//
// Boundedness anchor: the GET read path runs through apiRequestWithRetry
// with the default maxAttempts of 3. A retryable status (503/429/TIMEOUT)
// is replayed at most twice → exactly 3 round-trips; a non-retryable 500 is
// surfaced on the first round-trip. "Does not retry forever" is asserted
// directly off the MockCall ledger (or the injected-fetch counter), never
// inferred.

import { describe, test, expect } from "bun:test";

import { runCli } from "../src/cli.ts";
import { bufferStream, withAuthedStateDir } from "./helpers-cli-state.ts";
import { withMockApi, jsonResponse, type MockRoutes } from "./helpers-mock-api.ts";

const WS_ID = "01900000-0000-7000-8000-000000fa17e1" as const;
const READ_PATH = `/v1/workspaces/${WS_ID}/agents` as const;
const READ_ROUTE = `GET ${READ_PATH}` as const;

// Effect-shape contract (mirrors failure-modes.integration.test.ts):
//   HTTP 4xx/5xx → ServerError → exit 3.
//   fetch-level connection failure → NetworkError → exit 2.
const EXIT_SERVER_ERROR = 3 as const;
const EXIT_NETWORK_ERROR = 2 as const;

// apiRequestWithRetry's DEFAULT_MAX_ATTEMPTS. A retryable GET is replayed
// twice → exactly this many round-trips. Mirrored locally so the bound is
// asserted against a named constant, not a magic 3.
const DEFAULT_MAX_ATTEMPTS = 3 as const;
const SINGLE_ROUND_TRIP = 1 as const;

const REQUEST_ID = "req_read_path_test" as const;

// Note on backoff: the bounded-retry cases keep retries ON (we do NOT set
// AGENTSFLEET_NO_RETRY) precisely so the cap is exercised. The retry-delay
// knobs aren't exposed to the CLI path, so each retrying case pays the real
// ~750ms exponential backoff once. That's acceptable for an integration
// suite and keeps the transport honest end-to-end.

const authedRead = <T>(fn: (stateDir: string) => Promise<T>): Promise<T> =>
  withAuthedStateDir({ workspaceId: WS_ID, sessionId: "sess_read" }, fn);

function errorEnvelope(
  code: string,
  message: string,
): { error: { code: string; message: string }; request_id: string } {
  return { error: { code, message }, request_id: REQUEST_ID };
}

describe("read-path failures — server 5xx boundedness", () => {
  test("500 UZ-INTERNAL-002 is surfaced on the first round-trip and is NOT retried (500 ∉ retryable set)", async () => {
    await authedRead(async () => {
      const routes: MockRoutes = {
        [READ_ROUTE]: () =>
          jsonResponse(500, errorEnvelope("UZ-INTERNAL-002", "Database error")),
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(["list"], {
          stdout: out.stream,
          stderr: err.stream,
          env: { AGENTSFLEET_API_URL: apiUrl },
        });
        expect(code).toBe(EXIT_SERVER_ERROR);
        const text = err.read();
        expect(text).toContain("UZ-INTERNAL-002");
        expect(text).toContain("Database error");
        // 500 is a definite server-side outcome — replaying it risks acting
        // on a half-applied effect, so the client surfaces it immediately.
        expect(calls).toHaveLength(SINGLE_ROUND_TRIP);
      });
    });
  });

  test("503 UZ-API-002 is retried but BOUNDED — the mock sees exactly maxAttempts round-trips, then the code surfaces", async () => {
    await authedRead(async () => {
      const routes: MockRoutes = {
        [READ_ROUTE]: () =>
          jsonResponse(503, errorEnvelope("UZ-API-002", "Event-stream capacity reached")),
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(["list"], {
          stdout: out.stream,
          stderr: err.stream,
          env: { AGENTSFLEET_API_URL: apiUrl },
        });
        expect(code).toBe(EXIT_SERVER_ERROR);
        const text = err.read();
        expect(text).toContain("UZ-API-002");
        expect(text).toContain("Event-stream capacity reached");
        // The headline assertion: retries are bounded. A persistently failing
        // 503 stops at maxAttempts — it does NOT hammer the server forever.
        expect(calls).toHaveLength(DEFAULT_MAX_ATTEMPTS);
        // Every recorded call hit the read path (no stray routes).
        expect(calls.every((c) => c.method === "GET" && c.path === READ_PATH)).toBe(true);
      });
    });
  });
});

describe("read-path failures — 429 rate limit boundedness", () => {
  test("429 UZ-API-001 surfaces, exits non-zero, and is retried at most maxAttempts times", async () => {
    await authedRead(async () => {
      const routes: MockRoutes = {
        [READ_ROUTE]: () =>
          jsonResponse(429, errorEnvelope("UZ-API-001", "Too many in-flight requests")),
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(["list"], {
          stdout: out.stream,
          stderr: err.stream,
          env: { AGENTSFLEET_API_URL: apiUrl },
        });
        expect(code).toBe(EXIT_SERVER_ERROR);
        const text = err.read();
        expect(text).toContain("UZ-API-001");
        expect(text).toContain("Too many in-flight requests");
        // 429 is retryable (the request was shed, not processed) but still
        // bounded — a rate-limited client must not self-amplify the storm.
        expect(calls).toHaveLength(DEFAULT_MAX_ATTEMPTS);
      });
    });
  });
});

describe("read-path failures — request timeout boundedness", () => {
  test("a fetch that aborts surfaces a clean TIMEOUT message, exits non-zero, and the transport stops at maxAttempts", async () => {
    await authedRead(async () => {
      // The real production timeout fires an AbortController whose abort
      // surfaces from fetch as an AbortError; apiRequest rewraps that as
      // ApiError(code:"TIMEOUT"). Simulating the AbortError here exercises
      // that exact rewrap + the retry layer's timeout classification without
      // paying the 15s wall-clock timeout. We count fetch invocations to
      // prove the timeout path is bounded, not infinite.
      let fetchCalls = 0;
      const abortingFetch = (async (_url: string, _init?: RequestInit) => {
        fetchCalls += 1;
        const abortError = Object.assign(new Error("The operation was aborted"), {
          name: "AbortError",
        });
        throw abortError;
      }) as unknown as typeof fetch;

      const out = bufferStream();
      const err = bufferStream();
      const code = await runCli(["list"], {
        stdout: out.stream,
        stderr: err.stream,
        // apiUrl is irrelevant — abortingFetch never reaches the network.
        env: { AGENTSFLEET_API_URL: "http://127.0.0.1:1/v1" },
        fetchImpl: abortingFetch,
      });
      expect(code).toBe(EXIT_SERVER_ERROR);
      const text = err.read();
      // The operator must see a recognizable timeout, not a raw stack/abort.
      expect(text).toMatch(/timed out/i);
      expect(text).toContain("TIMEOUT");
      // Bounded: the timeout class is retryable but capped at maxAttempts.
      expect(fetchCalls).toBe(DEFAULT_MAX_ATTEMPTS);
    });
  });
});

describe("read-path failures — connection refused vs auth-required disambiguation", () => {
  test("ECONNREFUSED on an authed read yields a connection/network message — distinct from the unauthenticated path", async () => {
    // Valid creds are seeded, so this is NOT the auth-required branch. The
    // failure is purely transport: an unroutable address (port 1, reserved,
    // refuses immediately). The CLI must say "can't reach the API", never
    // "not authenticated / run login".
    await authedRead(async () => {
      const out = bufferStream();
      const err = bufferStream();
      const code = await runCli(["list"], {
        stdout: out.stream,
        stderr: err.stream,
        // Real globalThis.fetch (no fetchImpl) against a refused port.
        env: { AGENTSFLEET_API_URL: "http://127.0.0.1:1" },
      });
      // NetworkError → exit 2, distinct from AuthError's exit 1.
      expect(code).toBe(EXIT_NETWORK_ERROR);
      const text = err.read();
      // The message is about reachability/connectivity. The exact wording
      // varies by runtime: Node surfaces a TypeError("fetch failed") that
      // maps to "cannot reach agentsfleet API at <url>"; Bun surfaces
      // "Unable to connect. Is the computer able to access the url?". Either
      // way the operator is told it's a CONNECTION problem.
      expect(text).toMatch(/cannot reach|unable to connect|connectivity|access the url/i);
      // …and explicitly NOT the auth-required surface. This is the load-bearing
      // disambiguation: a refused connection must never be mistaken for an
      // expired/absent token, or the operator re-logs-in instead of fixing
      // the network.
      expect(text).not.toContain("not authenticated");
      expect(text.toLowerCase()).not.toContain("run `agentsfleet login`");
    });
  });
});
