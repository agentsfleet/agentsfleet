// API failure modes on the AUTHED read path (`agentsfleet fleet list`).
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
//   This file proves the wired read path sends each request once and
//   renders the right exit codes.
//
// Real UZ codes used here — declared in rustd/crates/foundation/af_core/src/error_code/,
// with the status each answers in rustd/crates/foundation/af_core/src/problem/. The
// detail strings below are this file's fixtures, not the registry's titles:
//   UZ-INTERNAL-002  (500, "Database error")            INTERNAL_DB_QUERY
//   UZ-API-002       (503, "Event-stream capacity …")   SSE_STREAM_CAP
//   UZ-API-001       (429, "Too many in-flight …")      API_BACKPRESSURE
//
// Boundedness anchor: the CLI sends a request once, so every failure below
// surfaces on the first round-trip, asserted directly off the MockCall ledger.

import { describe, test, expect } from "bun:test";

import { runCli } from "../src/cli.ts";
import { bufferStream, withAuthedStateDir, cliEnv } from "./helpers-cli-state.ts";
import { withMockApi, jsonResponse, type MockRoutes } from "./helpers-mock-api.ts";

const WS_ID = "01900000-0000-7000-8000-000000fa17e1" as const;
const READ_PATH = `/v1/workspaces/${WS_ID}/fleets` as const;
const READ_ROUTE = `GET ${READ_PATH}` as const;

// Effect-shape contract (mirrors failure-modes.integration.test.ts):
//   HTTP 4xx/5xx → ServerError → exit 3.
//   fetch-level connection failure → NetworkError → exit 2.
const EXIT_SERVER_ERROR = 3 as const;
const EXIT_NETWORK_ERROR = 2 as const;

const SINGLE_ROUND_TRIP = 1 as const;

const REQUEST_ID = "req_read_path_test" as const;

const authedRead = <T>(fn: (stateDir: string) => Promise<T>): Promise<T> =>
  withAuthedStateDir({ workspaceId: WS_ID, sessionId: "sess_read" }, fn);

function errorEnvelope(
  code: string,
  message: string,
): { error_code: string; detail: string; request_id: string } {
  return { error_code: code, detail: message, request_id: REQUEST_ID };
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
        const code = await runCli(["fleet", "list"], {
          stdout: out.stream,
          stderr: err.stream,
          env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }),
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

  test("503 UZ-API-002 surfaces on the first round-trip", async () => {
    await authedRead(async () => {
      const routes: MockRoutes = {
        [READ_ROUTE]: () =>
          jsonResponse(503, errorEnvelope("UZ-API-002", "Event-stream capacity reached")),
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(["fleet", "list"], {
          stdout: out.stream,
          stderr: err.stream,
          env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }),
        });
        expect(code).toBe(EXIT_SERVER_ERROR);
        const text = err.read();
        expect(text).toContain("UZ-API-002");
        expect(text).toContain("Event-stream capacity reached");
        expect(calls).toHaveLength(SINGLE_ROUND_TRIP);
        // Every recorded call hit the read path (no stray routes).
        expect(calls.every((c) => c.method === "GET" && c.path === READ_PATH)).toBe(true);
      });
    });
  });
});

describe("read-path failures — 429 rate limit boundedness", () => {
  test("429 UZ-API-001 surfaces on the first round-trip and exits non-zero", async () => {
    await authedRead(async () => {
      const routes: MockRoutes = {
        [READ_ROUTE]: () =>
          jsonResponse(429, errorEnvelope("UZ-API-001", "Too many in-flight requests")),
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(["fleet", "list"], {
          stdout: out.stream,
          stderr: err.stream,
          env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }),
        });
        expect(code).toBe(EXIT_SERVER_ERROR);
        const text = err.read();
        expect(text).toContain("UZ-API-001");
        expect(text).toContain("Too many in-flight requests");
        expect(calls).toHaveLength(SINGLE_ROUND_TRIP);
      });
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
      const code = await runCli(["fleet", "list"], {
        stdout: out.stream,
        stderr: err.stream,
        // Real globalThis.fetch (no fetchImpl) against a refused port.
        env: cliEnv({ AGENTSFLEET_API_URL: "http://127.0.0.1:1" }),
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

describe("an unreadable state file warns and continues as logged out", () => {
  // A missing or corrupt file already reads as logged-out inside the store;
  // this is the OTHER class — the file exists but cannot be read (here: it
  // is a directory, the portable stand-in for EACCES/EIO). Silently folding
  // that into "logged out" sends the operator to re-login against a store
  // that cannot be read; the contract is warn-with-cause, then behave as
  // logged out so read-only commands still work.
  test("credentials.json unreadable → stderr warning names the file, command still runs", async () => {
    const { withFreshStateDir } = await import("./helpers-cli-state.ts");
    const fs = await import("node:fs/promises");
    const path = await import("node:path");
    await withFreshStateDir(async (stateDir) => {
      // A directory where the file should be: readFile → EISDIR, not ENOENT.
      await fs.mkdir(path.join(stateDir, "credentials.json"), { recursive: true });
      const out = bufferStream();
      const err = bufferStream();
      // `--version` exits before the store loads; `secret list` runs the
      // load, then the auth guard sees the logged-out fallback and refuses.
      const code = await runCli(["secrets", "list"], {
        stdout: out.stream,
        stderr: err.stream,
        env: cliEnv(),
      });
      const warn = err.read();
      // One plain sentence: what broke, what it is doing instead, how to fix.
      expect(warn).toContain("cannot read your saved sign-in");
      expect(warn).toContain("credentials.json:");
      expect(warn).toContain("continuing signed out, against");
      expect(warn).toContain("agentsfleet login");
      // The errno, never the absolute path — that would carry the operator's
      // home directory and username into any CI log.
      expect(warn).not.toContain("/.config/agentsfleet");
      // The command then behaves exactly as logged out — refused by the
      // auth guard, not crashed by the unreadable file.
      expect(code).toBe(1);
      expect(warn).toContain("not authenticated");
    });
  });
});

describe("the workspaces arm reports its own file", () => {
  // The two arms share one `orWarn` closure, so bun reports state-load.ts at
  // 100% line coverage with only the credentials arm exercised — the floor
  // cannot see this gap. A copy-paste of the wrong filename or fallback ships
  // green and tells the operator the wrong file is unreadable.
  test("workspaces.json unreadable → the warning names THAT file, not credentials.json", async () => {
    const { withFreshStateDir } = await import("./helpers-cli-state.ts");
    const fs = await import("node:fs/promises");
    const path = await import("node:path");
    await withFreshStateDir(async (stateDir) => {
      await fs.mkdir(path.join(stateDir, "workspaces.json"), { recursive: true });
      const out = bufferStream();
      const err = bufferStream();
      const code = await runCli(["secrets", "list"], {
        stdout: out.stream,
        stderr: err.stream,
        env: cliEnv(),
      });
      const warn = err.read();
      expect(warn).toContain("workspaces.json:");
      expect(warn).not.toContain("credentials.json:");
      expect(warn).toContain("cannot read your saved sign-in");
      expect(code).toBe(1);
    });
  });
});
