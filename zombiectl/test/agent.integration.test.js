import { describe, test, expect } from "bun:test";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { Writable } from "node:stream";

import { runCli } from "../src/cli.js";
import { saveCredentials, saveWorkspaces } from "../src/lib/state.js";
import { withMockApi, jsonResponse } from "./helpers-mock-api.js";

const WS_ID = "ws_agent_test";
const ZOMBIE_ID = "zmb_agent_test";

function bufferStream() {
  let data = "";
  return {
    stream: new Writable({ write(chunk, _enc, cb) { data += String(chunk); cb(); } }),
    read: () => data,
  };
}

async function withAuthedStateDir(fn) {
  const previous = process.env.ZOMBIE_STATE_DIR;
  const dir = await fs.mkdtemp(path.join(os.tmpdir(), "zombiectl-agent-"));
  process.env.ZOMBIE_STATE_DIR = dir;
  try {
    await saveCredentials({
      token: "header.payload.sig",
      saved_at: Date.now(),
      session_id: "sess_agent",
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

describe("agent (external API key) commands", () => {
  test("`agent add` POSTs the new key and prints the raw value exactly once (shown-once contract)", async () => {
    await withAuthedStateDir(async () => {
      let postBody = null;
      const routes = {
        [`POST /v1/workspaces/${WS_ID}/agent-keys`]: async (_req, _url, body) => {
          postBody = body;
          return jsonResponse(201, {
            agent_id: "agent_key_001",
            key: "zmb_test_raw_key_value_only_shown_once",
            created_at: Date.now(),
          });
        },
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          [
            "agent", "add",
            "--workspace", WS_ID,
            "--zombie", ZOMBIE_ID,
            "--name", "langgraph-bot",
            "--description", "external orchestration",
          ],
          { stdout: out.stream, stderr: err.stream, env: { ZOMBIE_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        const text = out.read();
        expect(text).toContain("agent_key_001");
        expect(text).toContain("zmb_test_raw_key_value_only_shown_once");
        // The shown-once warning must be present in non-JSON mode.
        expect(text).toMatch(/shown once/i);

        // POST body shape contract: zombie_id + name + description.
        const parsed = JSON.parse(postBody);
        expect(parsed.zombie_id).toBe(ZOMBIE_ID);
        expect(parsed.name).toBe("langgraph-bot");
        expect(parsed.description).toBe("external orchestration");

        expect(calls.map((c) => c.method)).toEqual(["POST"]);
      });
    });
  });

  test("`agent list` GETs the workspace's external agent keys and prints a table", async () => {
    await withAuthedStateDir(async () => {
      const routes = {
        [`GET /v1/workspaces/${WS_ID}/agent-keys`]: () => jsonResponse(200, {
          items: [
            { agent_id: "agent_a", name: "langgraph-bot", description: "alpha", last_used_at: 1700000000000 },
            { agent_id: "agent_b", name: "crewai-bot",    description: "beta",  last_used_at: null },
          ],
        }),
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["agent", "list", "--workspace", WS_ID],
          { stdout: out.stream, stderr: err.stream, env: { ZOMBIE_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        const text = out.read();
        expect(text).toContain("langgraph-bot");
        expect(text).toContain("crewai-bot");
        expect(text).toContain("agent_a");
        expect(text).toContain("agent_b");
        expect(text).toContain("never");  // last_used_at: null renders as "never"
        expect(calls.map((c) => `${c.method} ${c.path}`)).toEqual([
          `GET /v1/workspaces/${WS_ID}/agent-keys`,
        ]);
      });
    });
  });

  test("`agent delete <id>` DELETEs the key and prints invalidation confirmation", async () => {
    await withAuthedStateDir(async () => {
      const routes = {
        [`DELETE /v1/workspaces/${WS_ID}/agent-keys/agent_to_delete`]:
          () => jsonResponse(204, {}),
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["agent", "delete", "--workspace", WS_ID, "agent_to_delete"],
          { stdout: out.stream, stderr: err.stream, env: { ZOMBIE_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        expect(out.read()).toMatch(/agent_to_delete.*invalidated/i);
        expect(calls.map((c) => `${c.method} ${c.path}`)).toEqual([
          `DELETE /v1/workspaces/${WS_ID}/agent-keys/agent_to_delete`,
        ]);
      });
    });
  });
});
