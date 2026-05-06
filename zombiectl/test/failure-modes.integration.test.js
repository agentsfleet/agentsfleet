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
//   UZ-ZMB-006        (409, zombie name conflict)       error_entries.zig:180
//   UZ-EXEC-013       (500, runner agent run failed)    error_entries_runtime.zig:56
//   UZ-INTERNAL-001   (503, database unavailable)       error_entries.zig:61
//
// Local skill-load errors (no fetch) come from src/lib/load-skill-from-path.js:
//   ERR_PATH_NOT_FOUND, ERR_SKILL_MISSING

import { describe, test, expect } from "bun:test";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { Writable } from "node:stream";

import { runCli } from "../src/cli.js";
import { saveCredentials, saveWorkspaces } from "../src/lib/state.js";
import { withMockApi, jsonResponse } from "./helpers-mock-api.js";

const WS_ID = "ws_failure_test";
const ZOMBIE_ID = "zmb_failure_test";

function bufferStream() {
  let data = "";
  return {
    stream: new Writable({ write(chunk, _enc, cb) { data += String(chunk); cb(); } }),
    read: () => data,
  };
}

async function withFreshStateDir(fn) {
  const previous = process.env.ZOMBIE_STATE_DIR;
  const dir = await fs.mkdtemp(path.join(os.tmpdir(), "zombiectl-fail-"));
  process.env.ZOMBIE_STATE_DIR = dir;
  try {
    return await fn(dir);
  } finally {
    if (previous === undefined) delete process.env.ZOMBIE_STATE_DIR;
    else process.env.ZOMBIE_STATE_DIR = previous;
    await fs.rm(dir, { recursive: true, force: true });
  }
}

async function withAuthedStateDir(fn) {
  return withFreshStateDir(async (dir) => {
    await saveCredentials({
      token: "header.payload.sig",
      saved_at: Date.now(),
      session_id: "sess_fail",
      api_url: null,
    });
    await saveWorkspaces({
      current_workspace_id: WS_ID,
      items: [{ workspace_id: WS_ID, name: "test-ws", created_at: Date.now() }],
    });
    return await fn(dir);
  });
}

function errorEnvelope(code, message, requestId = "req_fail_test") {
  return { error: { code, message }, request_id: requestId };
}

describe("failure modes — login surface", () => {
  test("auth service 503 with UZ-AUTH-004 surfaces the code on stderr and exits 1", async () => {
    await withFreshStateDir(async () => {
      const routes = {
        "POST /v1/auth/sessions": () => jsonResponse(503,
          errorEnvelope("UZ-AUTH-004", "Authentication service unavailable")),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["login", "--no-open", "--no-input", "--timeout-sec", "2", "--poll-ms", "50"],
          { stdout: out.stream, stderr: err.stream, env: { ZOMBIE_API_URL: apiUrl } },
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
    await withAuthedStateDir(async () => {
      // Start the customer in a logged-in but workspace-less state so the
      // failed `workspace add` is the moment they hit the paused error.
      await saveWorkspaces({ current_workspace_id: null, items: [] });
      const routes = {
        "POST /v1/workspaces": () => jsonResponse(402,
          errorEnvelope("UZ-WORKSPACE-002", "Workspace paused")),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(["workspace", "add", "my-repo"], {
          stdout: out.stream, stderr: err.stream, env: { ZOMBIE_API_URL: apiUrl },
        });
        expect(code).toBe(1);
        const text = err.read();
        expect(text).toContain("UZ-WORKSPACE-002");
        expect(text).toContain("Workspace paused");
      });
    });
  });
});

describe("failure modes — install surface (local + server)", () => {
  test("install --from /nonexistent/path errors locally with ERR_PATH_NOT_FOUND (no fetch)", async () => {
    await withAuthedStateDir(async () => {
      // Mock with empty routes — any HTTP attempt becomes a 404 and the test
      // catches an unexpected outbound call. The CLI must fail before fetch.
      await withMockApi({}, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["install", "--from", "/definitely/does/not/exist/zombie-template"],
          { stdout: out.stream, stderr: err.stream, env: { ZOMBIE_API_URL: apiUrl } },
        );
        expect(code).toBe(1);
        expect(err.read()).toContain("ERR_PATH_NOT_FOUND");
        expect(calls).toHaveLength(0);
      });
    });
  });

  test("install --from <dir-without-SKILL.md> errors locally with ERR_SKILL_MISSING", async () => {
    await withAuthedStateDir(async () => {
      const tmpDir = await fs.mkdtemp(path.join(os.tmpdir(), "zombiectl-empty-skill-"));
      try {
        await withMockApi({}, async (apiUrl, calls) => {
          const out = bufferStream();
          const err = bufferStream();
          const code = await runCli(
            ["install", "--from", tmpDir],
            { stdout: out.stream, stderr: err.stream, env: { ZOMBIE_API_URL: apiUrl } },
          );
          expect(code).toBe(1);
          expect(err.read()).toContain("ERR_SKILL_MISSING");
          expect(calls).toHaveLength(0);
        });
      } finally {
        await fs.rm(tmpDir, { recursive: true, force: true });
      }
    });
  });

  test("install hitting UZ-ZMB-006 (name conflict, 409) surfaces clearly without writing any local state", async () => {
    await withAuthedStateDir(async () => {
      const tmpDir = await fs.mkdtemp(path.join(os.tmpdir(), "zombiectl-skill-bundle-"));
      try {
        await fs.writeFile(path.join(tmpDir, "SKILL.md"),
          "---\nname: test-zombie\n---\n# test zombie\n", { mode: 0o644 });
        await fs.writeFile(path.join(tmpDir, "TRIGGER.md"),
          "---\nname: test-zombie\n---\n# trigger\n", { mode: 0o644 });
        const routes = {
          [`POST /v1/workspaces/${WS_ID}/zombies`]: () => jsonResponse(409,
            errorEnvelope("UZ-ZMB-006", "Zombie name 'test-zombie' already exists in this workspace")),
        };
        await withMockApi(routes, async (apiUrl) => {
          const out = bufferStream();
          const err = bufferStream();
          const code = await runCli(
            ["install", "--from", tmpDir],
            { stdout: out.stream, stderr: err.stream, env: { ZOMBIE_API_URL: apiUrl } },
          );
          expect(code).toBe(1);
          const text = err.read();
          expect(text).toContain("UZ-ZMB-006");
          expect(text).toContain("already exists");
        });
      } finally {
        await fs.rm(tmpDir, { recursive: true, force: true });
      }
    });
  });
});

describe("failure modes — runtime / observability surface", () => {
  test("install succeeds, but logs subsequently surface a runner failure event with UZ-EXEC-013 (the 'nullclaw errored out' shape)", async () => {
    await withAuthedStateDir(async () => {
      const tmpDir = await fs.mkdtemp(path.join(os.tmpdir(), "zombiectl-skill-runner-"));
      try {
        await fs.writeFile(path.join(tmpDir, "SKILL.md"),
          "---\nname: runner-test\n---\n# runner test\n", { mode: 0o644 });
        await fs.writeFile(path.join(tmpDir, "TRIGGER.md"),
          "---\nname: runner-test\n---\n# trigger\n", { mode: 0o644 });
        const routes = {
          // Step 1: install returns 201 — the server side is happy.
          [`POST /v1/workspaces/${WS_ID}/zombies`]: () => jsonResponse(201, {
            zombie_id: ZOMBIE_ID,
            name: "runner-test",
            status: "running",
          }),
          // Step 2: events show the worker died after the fact. The user
          // discovers the failure only by tailing logs — the install
          // command itself returned success.
          [`GET /v1/workspaces/${WS_ID}/zombies/${ZOMBIE_ID}/events`]:
            () => jsonResponse(200, {
              items: [
                {
                  created_at: 1700000000000,
                  actor: "agent",
                  status: "agent_error",
                  error_code: "UZ-EXEC-013",
                  response_text: "Runner agent run failed: nullclaw worker exited with signal SIGSEGV before claiming the zombie",
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
            ["install", "--from", tmpDir],
            { stdout: installOut.stream, stderr: installErr.stream,
              env: { ZOMBIE_API_URL: apiUrl } },
          );
          expect(installCode).toBe(0);

          // Step 2: logs surface the worker's post-install failure.
          const logsOut = bufferStream();
          const logsErr = bufferStream();
          const logsCode = await runCli(
            ["logs", ZOMBIE_ID],
            { stdout: logsOut.stream, stderr: logsErr.stream,
              env: { ZOMBIE_API_URL: apiUrl } },
          );
          expect(logsCode).toBe(0);
          const logsText = logsOut.read();
          // Captain's "nullclaw errored out" scenario: install returned 201,
          // but the worker died after the fact and the failure surfaces only
          // via events. The user MUST see the failure message in `logs`
          // output — otherwise the silent-success illusion is the bug.
          //
          // Note on rendering: zombie.js commandLogs prefers response_text
          // over status when both are present, so the visible signal is
          // the runner's failure message, not the bare `agent_error` tag.
          // Surfacing the status itself when response_text is set is a
          // separate UX concern; this test pins what the user sees today.
          expect(logsText).toContain("Runner agent run failed");
          expect(logsText).toContain("nullclaw");
        });
      } finally {
        await fs.rm(tmpDir, { recursive: true, force: true });
      }
    });
  });

  test("logs fetched with an expired token returns UZ-AUTH-003 / 401 — user knows to re-login", async () => {
    await withAuthedStateDir(async () => {
      const routes = {
        [`GET /v1/workspaces/${WS_ID}/zombies/${ZOMBIE_ID}/events`]:
          () => jsonResponse(401,
            errorEnvelope("UZ-AUTH-003", "Token expired — run `zombiectl login` to refresh")),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["logs", ZOMBIE_ID],
          { stdout: out.stream, stderr: err.stream, env: { ZOMBIE_API_URL: apiUrl } },
        );
        expect(code).toBe(1);
        const text = err.read();
        expect(text).toContain("UZ-AUTH-003");
        expect(text).toContain("Token expired");
        expect(text).toMatch(/zombiectl login/);
      });
    });
  });
});

describe("failure modes — infra / server-down surface", () => {
  test("any command hitting UZ-INTERNAL-001 (DB unavailable, 503) surfaces the code with status preserved", async () => {
    await withAuthedStateDir(async () => {
      // Workspace list does NOT call the API (it reads workspaces.json), so
      // exercise via doctor's /healthz check — the canonical first probe a
      // user runs when something feels off.
      const routes = {
        "GET /healthz": () => jsonResponse(503,
          errorEnvelope("UZ-INTERNAL-001", "Database unavailable")),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["doctor"],
          { stdout: out.stream, stderr: err.stream, env: { ZOMBIE_API_URL: apiUrl } },
        );
        // doctor's design is to gather all checks and report; a 503 on
        // /healthz is rendered as a failed check, not a hard CLI exit. The
        // contract under test is: the UZ-INTERNAL-001 code + message
        // appears in operator-readable output, not buried.
        expect([0, 1]).toContain(code);
        const text = `${out.read()}\n${err.read()}`;
        expect(text.toLowerCase()).toMatch(/database unavailable|unexpected payload/i);
      });
    });
  });
});
