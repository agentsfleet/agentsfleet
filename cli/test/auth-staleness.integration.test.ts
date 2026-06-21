// Auth lifecycle *beyond* the login handshake — what happens after a token
// has been minted and written to credentials.json, when that token later
// goes bad or the on-disk file is corrupt. failure-modes.integration.test.ts
// already pins the login-time 503 (UZ-AUTH-004) and a single expired-token
// read on `logs` (UZ-AUTH-003 via the events endpoint); this file extends the
// *uncovered* edges and shares no scenario with it:
//
//   (a) expired/invalid token surfaced on a different read verb (`list`),
//       asserting the no-loop + no-stack-trace + re-login-hint invariants
//       that the existing single-call `logs` test never checks.
//   (b) a token that is revoked mid-session — first call 200, second 401 —
//       proving the second read fails cleanly rather than wedging.
//   (c) corrupt / malformed credentials.json (truncated JSON, missing token
//       field, wrong 0644 perms) — graceful typed error, non-zero, no crash,
//       and the user's bytes are NOT silently rewritten or wiped.
//
// Codes are the real ones from the Zig registry
// (src/agentsfleetd/errors/error_entries.zig):
//   UZ-AUTH-003  (401, token expired)  error_entries.zig:62
//
// Transport contract verified live against api-dev: an HTTP 401 surfaces as a
// ServerError → exit 3 with the UZ code on stderr; the retry classifier treats
// 401 as fatal, so a bad token causes exactly one round-trip (no loop).

import { describe, test, expect } from "bun:test";
import fs from "node:fs/promises";
import path from "node:path";

import { runCli } from "../src/cli.ts";
import { bufferStream, withAuthedStateDir, withFreshStateDir } from "./helpers-cli-state.ts";
import { withMockApi, jsonResponse, type MockRoutes } from "./helpers-mock-api.ts";

const WS_ID = "01900000-0000-7000-8000-00000000a571";

// HTTP 4xx → ServerError → exit 3 (the Effect-shape contract the sibling
// failure-modes suite documents; pinned here so a regression to exit 1 trips).
const EXIT_SERVER_ERROR = 3 as const;
// Auth-guard bounce (no usable token) → exit 1.
const EXIT_AUTH_REQUIRED = 1 as const;

const CREDENTIALS_FILE = "credentials.json" as const;
const AGENTSFLEET_LOGIN_HINT = /agentsfleet login/;
// A Node/V8 stack-frame line is `    at <fn> (<file>:<line>:<col>)`. If any
// leaks onto stderr the CLI has surfaced a raw throw instead of a typed error.
const STACK_FRAME = /^\s*at\s+/m;

const LIST_ROUTE = `GET /v1/workspaces/${WS_ID}/fleets` as const;

const expiredEnvelope = {
  error: { code: "UZ-AUTH-003", message: "Token expired — run `agentsfleet login` to refresh" },
  request_id: "req_stale_test",
} as const;

const authedScope = <T>(fn: (stateDir: string) => Promise<T>): Promise<T> =>
  withAuthedStateDir({ workspaceId: WS_ID, sessionId: "sess_stale" }, fn);

function countCalls(calls: ReadonlyArray<{ method: string; path: string }>, route: string): number {
  const [method, pathname] = route.split(" ");
  return calls.filter((c) => c.method === method && c.path === pathname).length;
}

describe("auth staleness — expired token on a read", () => {
  test("`list` against a 401/UZ-AUTH-003 exits non-zero, hints re-login, leaks no stack frames, does not loop", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [LIST_ROUTE]: () => jsonResponse(401, expiredEnvelope),
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(["list"], {
          stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl },
        });
        expect(code).toBe(EXIT_SERVER_ERROR);
        const text = err.read();
        expect(text).toContain("UZ-AUTH-003");
        expect(text).toContain("Token expired");
        // The re-login recovery path must be surfaced so the operator knows
        // the fix is `agentsfleet login`, not a retry.
        expect(text).toMatch(AGENTSFLEET_LOGIN_HINT);
        // Typed error, not a raw throw: no V8 stack frames on stderr.
        expect(text).not.toMatch(STACK_FRAME);
        // 401 is fatal in the retry classifier — exactly one round-trip.
        expect(countCalls(calls, LIST_ROUTE)).toBe(1);
      });
    });
  });
});

describe("auth staleness — token revoked mid-session", () => {
  test("first `list` ok (200), a later `list` 401s cleanly (server-side revocation)", async () => {
    await authedScope(async () => {
      let hit = 0;
      const routes: MockRoutes = {
        [LIST_ROUTE]: () => {
          hit += 1;
          return hit === 1
            ? jsonResponse(200, { items: [], next_cursor: null })
            : jsonResponse(401, expiredEnvelope);
        },
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        // Leg 1: token still valid → success.
        const firstOut = bufferStream();
        const firstErr = bufferStream();
        const firstCode = await runCli(["list"], {
          stdout: firstOut.stream, stderr: firstErr.stream, env: { AGENTSFLEET_API_URL: apiUrl },
        });
        expect(firstCode).toBe(0);

        // Leg 2: token revoked server-side → clean typed failure, no loop.
        const secondOut = bufferStream();
        const secondErr = bufferStream();
        const secondCode = await runCli(["list"], {
          stdout: secondOut.stream, stderr: secondErr.stream, env: { AGENTSFLEET_API_URL: apiUrl },
        });
        expect(secondCode).toBe(EXIT_SERVER_ERROR);
        const text = secondErr.read();
        expect(text).toContain("UZ-AUTH-003");
        expect(text).not.toMatch(STACK_FRAME);
        // Exactly two round-trips total: the good one + the revoked one.
        expect(countCalls(calls, LIST_ROUTE)).toBe(2);
      });
    });
  });
});

describe("auth staleness — corrupt credentials.json", () => {
  // Each leg writes a deliberately-bad credentials.json directly to disk, runs
  // an auth-required command, and asserts (1) graceful non-zero exit, (2) no
  // crash / stack trace, and (3) the on-disk bytes are byte-identical after the
  // failed read — the CLI must never silently rewrite or wipe the user's file.
  // No HTTP route is needed: the auth-guard bounces before any fetch when the
  // file yields no usable token, so any outbound call would itself be the bug.

  async function expectGracefulNoWipe(
    raw: string,
    mode: number,
  ): Promise<void> {
    await withFreshStateDir(async (stateDir) => {
      const file = path.join(stateDir, CREDENTIALS_FILE);
      await fs.writeFile(file, raw, { mode });
      const before = await fs.readFile(file);

      await withMockApi({}, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(["list"], {
          stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl },
        });
        // No usable token → auth-guard bounce → exit 1, never a crash.
        expect(code).toBe(EXIT_AUTH_REQUIRED);
        const text = err.read();
        expect(text).toMatch(AGENTSFLEET_LOGIN_HINT);
        expect(text).not.toMatch(STACK_FRAME);
        // The guard fires before transport — no request should ever leave.
        expect(calls).toHaveLength(0);
      });

      // The failed read must not have touched the user's bytes.
      const after = await fs.readFile(file);
      expect(after.equals(before)).toBe(true);
    });
  }

  test("truncated JSON → graceful auth bounce, file bytes unchanged", async () => {
    // readJson swallows SyntaxError into the null-token fallback; the file is
    // never repaired or cleared, so a hand-edited / interrupted-write file is
    // preserved for the user to inspect.
    await expectGracefulNoWipe('{"token":"header.payload.si', 0o600);
  });

  test("missing token field → graceful auth bounce, file bytes unchanged", async () => {
    // Valid JSON, but no `token` key → loadCredentials yields token:null →
    // auth-guard bounce. The rest of the object is left intact on disk.
    await expectGracefulNoWipe(
      `${JSON.stringify({ saved_at: 123, session_id: "sess_x", api_url: null }, null, 2)}\n`,
      0o600,
    );
  });

  test("wrong 0644 perms with no token → graceful auth bounce, file bytes unchanged", async () => {
    // A world-readable credentials.json with token:null. The CLI reads it (perms
    // are not enforced on read), finds no token, and bounces — without silently
    // re-writing the file (which would also re-tighten the perms and mask the
    // misconfiguration from the user).
    await expectGracefulNoWipe(
      `${JSON.stringify({ token: null, saved_at: 456, session_id: null, api_url: null }, null, 2)}\n`,
      0o644,
    );
  });
});
