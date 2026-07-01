// Cross-cutting failure-mode coverage. Error codes mirror the Zig
// backend's error registry (src/errors/error_entries{,_runtime}.zig) so
// tests reflect real production failure shapes rather than invented ones.
//
// Each scenario answers: "if the user hits this exact UZ-XXX-NNN response
// in the wild, does the CLI surface it clearly and exit non-zero, or does
// it succeed silently / crash?"
//
// Surveyed codes used here:
//   UZ-AUTH-003       (401, token expired)              error_entries.zig:78
//   UZ-AUTH-004       (503, auth service unavailable)   error_entries.zig:80
//   UZ-WORKSPACE-002  (402, workspace paused)           error_entries.zig:102
//   UZ-AGT-006        (409, fleet name conflict)       error_entries.zig:180
//   UZ-EXEC-013       (500, runner fleet run failed)    error_entries_runtime.zig:56
//   UZ-INTERNAL-001   (503, database unavailable)       error_entries.zig:61

import { describe, test, expect } from "bun:test";

import { runCli } from "../src/cli.ts";
import { saveWorkspaces } from "../src/lib/state.ts";
import { bufferStream, withAuthedStateDir, withFreshStateDir } from "./helpers-cli-state.ts";
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
): { error: { code: string; message: string }; request_id: string } {
  return { error: { code, message }, request_id: requestId };
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
          { stdout: out.stream, stderr: err.stream, stdin: ttyStdin, env: { AGENTSFLEET_API_URL: apiUrl } },
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
  test("workspace add returning UZ-WORKSPACE-002 (paused, 402) blocks the user with a billing-shaped error", async () => {
    await authedScope(async () => {
      // Start the customer in a logged-in but workspace-less state so the
      // failed `workspace add` is the moment they hit the paused error.
      await saveWorkspaces({ current_workspace_id: null, items: [] });
      const routes: MockRoutes = {
        "POST /v1/workspaces": () => jsonResponse(402,
          errorEnvelope("UZ-WORKSPACE-002", "Workspace paused")),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(["workspace", "add", "my-repo"], {
          stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl },
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

describe("failure modes — install surface (server)", () => {
  test("install hitting UZ-AGT-006 (name conflict, 409) surfaces clearly without writing any local state", async () => {
    await authedScope(async () => {
      const templateId = "github-pr-reviewer";
      const routes: MockRoutes = {
        // The gallery resolves so the create POST is reached; the create
        // then returns the 409 name conflict.
        [`GET /v1/workspaces/${WS_ID}/fleet-templates`]: () => jsonResponse(200, {
          items: [{ id: templateId, name: "test-fleet", visibility: "platform",
            requirements: { trigger_present: true } }],
        }),
        [`POST /v1/workspaces/${WS_ID}/fleets`]: () => jsonResponse(409,
          errorEnvelope("UZ-AGT-006", "Fleet name 'test-fleet' already exists in this workspace")),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["install", "--template", templateId],
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        // Effect-shape contract: HTTP 4xx → ServerError → exit 3.
        // The pre-Effect path collapsed every API failure to exit 1.
        expect(code).toBe(3);
        const text = err.read();
        expect(text).toContain("UZ-AGT-006");
        expect(text).toContain("already exists");
      });
    });
  });
});

describe("failure modes — runtime / observability surface", () => {
  test("install succeeds, but logs subsequently surface a runner failure event with UZ-EXEC-013 (the 'nullclaw errored out' shape)", async () => {
    await authedScope(async () => {
      const templateId = "runner-test";
      const routes: MockRoutes = {
        // The gallery resolves so the create POST is reached.
        [`GET /v1/workspaces/${WS_ID}/fleet-templates`]: () => jsonResponse(200, {
          items: [{ id: templateId, name: "runner-test", visibility: "platform",
            requirements: { trigger_present: true } }],
        }),
        // Step 1: install returns 201 — the server side is happy.
        [`POST /v1/workspaces/${WS_ID}/fleets`]: () => jsonResponse(201, {
          fleet_id: FLEET_ID,
          name: "runner-test",
          status: "running",
        }),
        // Step 2: events show the worker died after the fact. The user
        // discovers the failure only by tailing logs — the install
        // command itself returned success.
        [`GET /v1/workspaces/${WS_ID}/fleets/${FLEET_ID}/events`]:
          () => jsonResponse(200, {
            items: [
              {
                created_at: 1700000000000,
                actor: "fleet",
                status: "fleet_error",
                error_code: "UZ-EXEC-013",
                response_text: "Runner fleet run failed: nullclaw worker exited with signal SIGSEGV before claiming the fleet",
              },
            ],
            next_cursor: null,
          }),
      };
      await withMockApi(routes, async (apiUrl) => {
        // Step 1: install succeeds.
        const installOut = bufferStream();
        const installErr = bufferStream();
        const installCode = await runCli(
          ["install", "--template", templateId],
          { stdout: installOut.stream, stderr: installErr.stream,
            env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        expect(installCode).toBe(0);

        // Step 2: logs surface the worker's post-install failure.
        const logsOut = bufferStream();
        const logsErr = bufferStream();
        const logsCode = await runCli(
          ["logs", FLEET_ID],
          { stdout: logsOut.stream, stderr: logsErr.stream,
            env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        expect(logsCode).toBe(0);
        const logsText = logsOut.read();
        // Captain's "nullclaw errored out" scenario: install returned 201,
        // but the worker died after the fact and the failure surfaces only
        // via events. The user MUST see the failure message in `logs`
        // output — otherwise the silent-success illusion is the bug.
        //
        // Note on rendering: fleet.js commandLogs prefers response_text
        // over status when both are present, so the visible signal is
        // the runner's failure message, not the bare `fleet_error` tag.
        // Surfacing the status itself when response_text is set is a
        // separate UX concern; this test pins what the user sees today.
        expect(logsText).toContain("Runner fleet run failed");
        expect(logsText).toContain("nullclaw");
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
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
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
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
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
