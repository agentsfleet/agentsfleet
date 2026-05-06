import { describe, test, expect } from "bun:test";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { Writable } from "node:stream";

import { runCli } from "../src/cli.js";
import { saveCredentials, saveWorkspaces } from "../src/lib/state.js";
import { withMockApi, jsonResponse } from "./helpers-mock-api.js";

const WS_ID = "ws_cred_test";

function bufferStream() {
  let data = "";
  return {
    stream: new Writable({ write(chunk, _enc, cb) { data += String(chunk); cb(); } }),
    read: () => data,
  };
}

async function withAuthedStateDir(fn) {
  const previous = process.env.ZOMBIE_STATE_DIR;
  const dir = await fs.mkdtemp(path.join(os.tmpdir(), "zombiectl-cred-"));
  process.env.ZOMBIE_STATE_DIR = dir;
  try {
    await saveCredentials({
      token: "header.payload.sig",
      saved_at: Date.now(),
      session_id: "sess_cred",
      api_url: null,
    });
    await saveWorkspaces({
      current_workspace_id: WS_ID,
      items: [{ workspace_id: WS_ID, name: "test-ws", created_at: Date.now() }],
    });
    return await fn();
  } finally {
    if (previous === undefined) delete process.env.ZOMBIE_STATE_DIR;
    else process.env.ZOMBIE_STATE_DIR = previous;
    await fs.rm(dir, { recursive: true, force: true });
  }
}

describe("credential commands", () => {
  test("`credential add` with no existing credential GETs list (empty), POSTs the secret, prints stored", async () => {
    await withAuthedStateDir(async () => {
      let postBody = null;
      const routes = {
        [`GET /v1/workspaces/${WS_ID}/credentials`]: () => jsonResponse(200, { credentials: [] }),
        [`POST /v1/workspaces/${WS_ID}/credentials`]: async (_req, _url, body) => {
          postBody = body;
          return jsonResponse(201, { name: "github", created_at: Date.now() });
        },
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["credential", "add", "github", `--data={"token":"ghp_test_value"}`],
          { stdout: out.stream, stderr: err.stream, env: { ZOMBIE_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        expect(out.read()).toMatch(/stored/i);
        // Ledger: the skip-if-exists GET fires first, then the POST.
        expect(calls.map((c) => `${c.method} ${c.path}`)).toEqual([
          `GET /v1/workspaces/${WS_ID}/credentials`,
          `POST /v1/workspaces/${WS_ID}/credentials`,
        ]);
        // The POST body carries the name + opaque data object intact.
        const parsed = JSON.parse(postBody);
        expect(parsed.name).toBe("github");
        expect(parsed.data).toEqual({ token: "ghp_test_value" });
      });
    });
  });

  test("`credential add` skips silently when the name already exists (default upsert guard)", async () => {
    await withAuthedStateDir(async () => {
      const routes = {
        [`GET /v1/workspaces/${WS_ID}/credentials`]: () => jsonResponse(200, {
          credentials: [{ name: "github", created_at: 1700000000000 }],
        }),
        // No POST handler — if the CLI hits POST under default upsert, the mock
        // returns 404 and the test surfaces an unexpected call.
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["credential", "add", "github", `--data={"token":"ghp_will_not_be_sent"}`],
          { stdout: out.stream, stderr: err.stream, env: { ZOMBIE_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        expect(out.read()).toMatch(/already exists/i);
        // Only the preflight GET fired; no POST.
        expect(calls.filter((c) => c.method === "POST")).toHaveLength(0);
      });
    });
  });

  test("`credential add --force` skips the preflight GET and POSTs immediately as overwritten", async () => {
    await withAuthedStateDir(async () => {
      let postBody = null;
      const routes = {
        // No GET handler — if --force trips the preflight, this becomes a 404
        // and the CLI errors out, which the test catches.
        [`POST /v1/workspaces/${WS_ID}/credentials`]: async (_req, _url, body) => {
          postBody = body;
          return jsonResponse(200, { name: "github", created_at: Date.now() });
        },
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["credential", "add", "github", `--data={"token":"ghp_force"}`, "--force"],
          { stdout: out.stream, stderr: err.stream, env: { ZOMBIE_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        expect(out.read()).toMatch(/overwritten/i);
        expect(calls.map((c) => c.method)).toEqual(["POST"]);
        const parsed = JSON.parse(postBody);
        expect(parsed.data).toEqual({ token: "ghp_force" });
      });
    });
  });

  test("`credential list` GETs the vault and prints names without secret bytes", async () => {
    await withAuthedStateDir(async () => {
      const routes = {
        [`GET /v1/workspaces/${WS_ID}/credentials`]: () => jsonResponse(200, {
          credentials: [
            { name: "github", created_at: 1700000000000 },
            { name: "slack", created_at: 1700000000001 },
          ],
        }),
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["credential", "list"],
          { stdout: out.stream, stderr: err.stream, env: { ZOMBIE_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        const text = out.read();
        expect(text).toContain("github");
        expect(text).toContain("slack");
        // Negative assertion: secret-shaped substrings never appear in list output
        // (the API would never return secret bytes here, but locking it down so a
        // future regression that prints an unexpected field surfaces immediately).
        expect(text).not.toContain("ghp_");
        expect(text).not.toMatch(/token/i);
        expect(calls.map((c) => `${c.method} ${c.path}`)).toEqual([
          `GET /v1/workspaces/${WS_ID}/credentials`,
        ]);
      });
    });
  });
});
