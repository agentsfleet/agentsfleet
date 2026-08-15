// The positive arm of the injected-environment seam, proven by DIVERGENCE:
// the injected `io.env` names a state directory that is NOT the process one,
// and the run must read and write THERE. Every other in-process suite injects
// `stateDirEnv()` — the process value — so injected and process directories
// never differ, and a regression that silently drops `io.env` on the way to
// the store (the composition roots in cli.ts / handlers-bind.ts, or the
// `mainLayerFor` process-env default) would keep the whole suite green.
// These two tests are the ones that go red.

import { describe, test, expect } from "bun:test";
import fs from "node:fs/promises";
import { mkdtempSync } from "node:fs";
import os from "node:os";
import path from "node:path";

import { runCli } from "../src/cli.ts";
import { STATE_DIR_ENV } from "../src/lib/config-dir.ts";
import { loadWorkspaces, saveCredentials, saveWorkspaces } from "../src/lib/state.ts";
import { bufferStream, FIXTURE_CREDENTIAL } from "./helpers-cli-state.ts";
import { withMockApi, jsonResponse, type MockRoutes } from "./helpers-mock-api.ts";

const WS_ID = "01900000-0000-7000-8000-00000000d1fa";

function divergentDir(): string {
  return mkdtempSync(path.join(os.tmpdir(), "agentsfleet-divergent-"));
}

describe("runCli({ env }) reaches the store when it diverges from the process environment", () => {
  test("credentials seeded ONLY in the injected directory authenticate an authed command", async () => {
    const dirA = divergentDir();
    const injected = { [STATE_DIR_ENV]: dirA };
    try {
      await saveCredentials(injected, {
        token: FIXTURE_CREDENTIAL,
        saved_at: Date.now(),
        session_id: "sess_divergent",
        api_url: null,
        credential_id: null,
      });
      await saveWorkspaces(injected, {
        current_workspace_id: WS_ID,
        items: [{ workspace_id: WS_ID, name: null, created_at: 1 }],
      });
      const routes: MockRoutes = {
        [`GET /v1/workspaces/${WS_ID}/secrets`]: () =>
          jsonResponse(200, { secrets: [] }),
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(["secret", "list", "--json"], {
          stdout: out.stream,
          stderr: err.stream,
          env: { ...injected, AGENTSFLEET_API_URL: apiUrl },
        });
        // The process environment points at the suite's EMPTY preload dir, so
        // authenticating at all proves the injected directory was read.
        expect(code).toBe(0);
        expect(calls.length).toBeGreaterThan(0);
      });
    } finally {
      await fs.rm(dirA, { recursive: true, force: true });
    }
  });

  test("a state write lands in the injected directory, and not in the process one", async () => {
    const dirA = divergentDir();
    const injected = { [STATE_DIR_ENV]: dirA };
    try {
      const seeded = {
        current_workspace_id: null,
        items: [{ workspace_id: WS_ID, name: null, created_at: 1 }],
      };
      await saveWorkspaces(injected, seeded);
      const before = await loadWorkspaces(process.env);
      const out = bufferStream();
      const err = bufferStream();
      const code = await runCli(["workspace", "use", WS_ID], {
        stdout: out.stream,
        stderr: err.stream,
        env: { ...injected, AGENTSFLEET_API_KEY: "agt_t_test" },
      });
      expect(code).toBe(0);
      const inInjected = await loadWorkspaces(injected);
      expect(inInjected.current_workspace_id).toBe(WS_ID);
      const inProcess = await loadWorkspaces(process.env);
      expect(inProcess.current_workspace_id).toBe(before.current_workspace_id);
    } finally {
      await fs.rm(dirA, { recursive: true, force: true });
    }
  });
});
