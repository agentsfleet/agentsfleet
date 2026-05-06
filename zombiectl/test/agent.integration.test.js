import { describe, test, expect } from "bun:test";

import { runCli } from "../src/cli.js";
import { bufferStream, withAuthedStateDir } from "./helpers-cli-state.js";
import { withMockApi, jsonResponse } from "./helpers-mock-api.js";

const WS_ID = "ws_agent_test";
const ZOMBIE_ID = "zmb_agent_test";
const authedScope = (fn) => withAuthedStateDir({ workspaceId: WS_ID, sessionId: "sess_agent" }, fn);

describe("agent (external API key) commands", () => {
  test("`agent add` POSTs the new key and prints the raw value exactly once (shown-once contract)", async () => {
    await authedScope(async () => {
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
    await authedScope(async () => {
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
    await authedScope(async () => {
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
