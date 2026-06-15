import { describe, test, expect } from "bun:test";

import { runCli } from "../src/cli.ts";
import { bufferStream, withAuthedStateDir } from "./helpers-cli-state.ts";
import { withMockApi, jsonResponse, type MockRoutes } from "./helpers-mock-api.ts";

const WS_ID = "01900000-0000-7000-8000-000000a6e711";
const AGENTSFLEET_ID = "01900000-0000-7000-8000-000000a67e57";
const authedScope = <T>(fn: (stateDir: string) => Promise<T>): Promise<T> =>
  withAuthedStateDir({ workspaceId: WS_ID, sessionId: "sess_agent" }, fn);

describe("agent (external API key) commands", () => {
  test("`agent add` POSTs the new key and prints the raw value exactly once (shown-once contract)", async () => {
    await authedScope(async () => {
      let postBody: string | null = null;
      const routes: MockRoutes = {
        [`POST /v1/workspaces/${WS_ID}/agent-keys`]: async (_req, _url, body) => {
          postBody = body;
          return jsonResponse(201, {
            agent_key_id: "agent_key_001",
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
            "--zombie", AGENTSFLEET_ID,
            "--name", "langgraph-bot",
            "--description", "external orchestration",
          ],
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        const text = out.read();
        expect(text).toContain("agent_key_001");
        expect(text).toContain("zmb_test_raw_key_value_only_shown_once");
        // The shown-once warning must be present in non-JSON mode.
        expect(text).toMatch(/shown once/i);

        // POST body shape contract: zombie_id + name + description.
        const parsed = JSON.parse(postBody ?? "{}") as {
          zombie_id?: string;
          name?: string;
          description?: string;
        };
        expect(parsed.zombie_id).toBe(AGENTSFLEET_ID);
        expect(parsed.name).toBe("langgraph-bot");
        expect(parsed.description).toBe("external orchestration");

        expect(calls.map((c) => c.method)).toEqual(["POST"]);
      });
    });
  });

  test("`agent list` GETs the workspace's external agent keys and prints a table", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`GET /v1/workspaces/${WS_ID}/agent-keys`]: () => jsonResponse(200, {
          items: [
            { agent_key_id: "agent_a", name: "langgraph-bot", description: "alpha", last_used_at: 1700000000000 },
            { agent_key_id: "agent_b", name: "crewai-bot",    description: "beta",  last_used_at: null },
          ],
        }),
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["agent", "list", "--workspace", WS_ID],
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
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
      const routes: MockRoutes = {
        [`DELETE /v1/workspaces/${WS_ID}/agent-keys/01900000-0000-7000-8000-0000a6e7de7e`]:
          () => jsonResponse(204, {}),
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["agent", "delete", "--workspace", WS_ID, "01900000-0000-7000-8000-0000a6e7de7e"],
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        expect(out.read()).toMatch(/01900000-0000-7000-8000-0000a6e7de7e.*invalidated/i);
        expect(calls.map((c) => `${c.method} ${c.path}`)).toEqual([
          `DELETE /v1/workspaces/${WS_ID}/agent-keys/01900000-0000-7000-8000-0000a6e7de7e`,
        ]);
      });
    });
  });
});
