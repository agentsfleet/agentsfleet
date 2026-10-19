// Cross-cutting failure-mode coverage. Error codes mirror the daemon's
// error registry (rustd/crates/afd_core/src/error_code/) so tests reflect
// real production failure shapes rather than invented ones.
//
// Each scenario answers: "if the user hits this exact UZ-XXX-NNN response
// in the wild, does the CLI surface it clearly and exit non-zero, or does
// it succeed silently / crash?"
//
// Surveyed codes used here:
//   UZ-AUTH-003       (401, token expired)              error_code/auth.rs:34
//   UZ-AUTH-004       (503, auth service unavailable)   error_code/auth.rs:129
//   UZ-RUN-011        (runner lost its lease)           error_code/fleet.rs:59
//   UZ-INTERNAL-001   (503, database unavailable)       error_code/request.rs:191
//   UZ-WORKSPACE-002  (402, workspace paused)           not declared by the
//                     daemon; its case covers how any 402 envelope reads

import { describe, test, expect } from "bun:test";

import { runCli } from "../src/cli.ts";
import { saveWorkspaces } from "../src/lib/state.ts";
import { bufferStream, withAuthedStateDir, withFreshStateDir, cliEnv } from "./helpers-cli-state.ts";
import { withMockApi, jsonResponse, type MockRoutes } from "./helpers-mock-api.ts";

const WS_ID = "01900000-0000-7000-8000-000000fa17e1";
const FLEET_ID = "01900000-0000-7000-8000-000000fa17e2";
// Interactive-terminal stdin so `login` runs the device flow (where the
// auth-service 503 is surfaced) rather than the non-TTY direct-token resolve.
const ttyStdin = { isTTY: true } as unknown as NodeJS.ReadableStream;
const authedScope = <T>(fn: (stateDir: string) => Promise<T>): Promise<T> =>
  withAuthedStateDir({ workspaceId: WS_ID, sessionId: "sess_fail" }, fn);

function errorEnvelope(
  code: string,
  message: string,
  requestId = "req_fail_test",
): { error_code: string; detail: string; request_id: string } {
  return { error_code: code, detail: message, request_id: requestId };
}

describe("failure modes — login surface", () => {
  test("auth service 503 with UZ-AUTH-004 surfaces the code on stderr and exits 1", async () => {
    await withFreshStateDir(async () => {
      const routes: MockRoutes = {
        "POST /v1/auth/sessions": () => jsonResponse(503,
          errorEnvelope("UZ-AUTH-004", "Authentication service unavailable")),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["login", "--no-open", "--no-input"],
          { stdout: out.stream, stderr: err.stream, stdin: ttyStdin, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }) },
        );
        expect(code).toBe(1);
        const text = err.read();
        expect(text).toContain("UZ-AUTH-004");
        expect(text).toContain("Authentication service unavailable");
        expect(text).toMatch(/request_id/);
      });
    });
  });
});

describe("failure modes — workspace surface", () => {
  test("workspace create returning UZ-WORKSPACE-002 (paused, 402) blocks the user with a billing-shaped error", async () => {
    await authedScope(async () => {
      // Start the customer in a logged-in but workspace-less state so the
      // failed `workspace create` is the moment they hit the paused error.
      await saveWorkspaces(process.env, { current_workspace_id: null, items: [] });
      const routes: MockRoutes = {
        "POST /v1/workspaces": () => jsonResponse(402,
          errorEnvelope("UZ-WORKSPACE-002", "Workspace paused")),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(["workspace", "create", "my-repo"], {
          stdout: out.stream, stderr: err.stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }),
        });
        // Effect-shape contract: HTTP 4xx → ServerError → exit 3.
        // The pre-Effect path collapsed every API failure to exit 1.
        expect(code).toBe(3);
        const text = err.read();
        expect(text).toContain("UZ-WORKSPACE-002");
        expect(text).toContain("Workspace paused");
      });
    });
  });
});

describe("failure modes — runtime / observability surface", () => {
  test("a running fleet whose runner lost its lease surfaces UZ-RUN-011 through logs", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        // The fleet was saved and started without complaint; the runner died
        // afterwards, so the only place the operator can see it is the event
        // stream.
        [`GET /v1/workspaces/${WS_ID}/fleets/${FLEET_ID}/events`]:
          () => jsonResponse(200, {
            items: [
              {
                created_at: "2023-11-14T22:13:20Z",
                actor: "fleet",
                status: "fleet_error",
                error_code: "UZ-RUN-011",
                response_text: "Runner fleet run failed: the runner lost its lease before the run finished",
              },
            ],
            next_cursor: null,
          }),
      };
      await withMockApi(routes, async (apiUrl) => {
        const logsOut = bufferStream();
        const logsErr = bufferStream();
        const logsCode = await runCli(
          ["logs", FLEET_ID],
          { stdout: logsOut.stream, stderr: logsErr.stream,
            env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }) },
        );
        expect(logsCode).toBe(0);
        // `logs` prefers response_text over the bare `fleet_error` status
        // (src/commands/fleet_logs.ts), so the runner's own sentence is the
        // visible signal. Without it the failure would read as silence.
        const logsText = logsOut.read();
        expect(logsText).toContain("Runner fleet run failed");
        expect(logsText).toContain("lost its lease");
      });
    });
  });

  test("logs fetched with an expired token returns UZ-AUTH-003 / 401 — user knows to re-login", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`GET /v1/workspaces/${WS_ID}/fleets/${FLEET_ID}/events`]:
          () => jsonResponse(401,
            errorEnvelope("UZ-AUTH-003", "Token expired — run `agentsfleet login` to refresh")),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["logs", FLEET_ID],
          { stdout: out.stream, stderr: err.stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }) },
        );
        // Effect-shape contract: HTTP 401 → ServerError → exit 3.
        // The user-visible message still carries UZ-AUTH-003 + the
        // "run agentsfleet login" suggestion so the recovery path is
        // unchanged from the operator's POV.
        expect(code).toBe(3);
        const text = err.read();
        expect(text).toContain("UZ-AUTH-003");
        expect(text).toContain("Token expired");
        expect(text).toMatch(/agentsfleet login/);
      });
    });
  });
});

describe("failure modes — infra / server-down surface", () => {
  test("doctor with /healthz returning UZ-INTERNAL-001 (DB unavailable, 503) renders [FAIL] server_reachable + the message and exits 1", async () => {
    await authedScope(async () => {
      // Pin both probes:
      //   /healthz → 503 with UZ-INTERNAL-001 (the failure under test)
      //   workspace probe → 200 (so it's not a confounding second failure)
      // doctor returns 0 iff every check passes (core-ops.js:84,105). With one
      // failed check, it deterministically returns 1 — pinned strictly here.
      const routes: MockRoutes = {
        "GET /healthz": () => jsonResponse(503,
          errorEnvelope("UZ-INTERNAL-001", "Database unavailable")),
        [`GET /v1/workspaces/${WS_ID}/fleets`]: () => jsonResponse(200, { items: [] }),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["doctor"],
          { stdout: out.stream, stderr: err.stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }) },
        );
        expect(code).toBe(1);
        const text = out.read();
        // The structured failure renders as a concrete "[FAIL] server_reachable"
        // line plus the indented detail carrying the upstream error message.
        expect(text).toContain("[FAIL] server_reachable");
        expect(text).toContain("Database unavailable");
        // Closing summary names the failure ratio explicitly so operators
        // can grep for it in CI logs.
        expect(text).toMatch(/2\/3 checks passed/);
      });
    });
  });
});
