import { describe, test, expect } from "bun:test";

import { runCli } from "../src/cli.ts";
import { bufferStream, withAuthedStateDir } from "./helpers-cli-state.ts";
import { withMockApi, jsonResponse, type MockRoutes } from "./helpers-mock-api.ts";

const WS_ID = "01900000-0000-7000-8000-000000a6e711";
const FLEET_ID = "01900000-0000-7000-8000-000000a67e57";
const authedScope = <T>(fn: (stateDir: string) => Promise<T>): Promise<T> =>
  withAuthedStateDir({ workspaceId: WS_ID, sessionId: "sess_agent" }, fn);

describe("fleet (external API key) commands", () => {
  test("`fleet-key create` POSTs the new key and prints the raw value exactly once (shown-once rule)", async () => {
    await authedScope(async () => {
      let postBody: string | null = null;
      const routes: MockRoutes = {
        [`POST /v1/workspaces/${WS_ID}/fleet-keys`]: async (_req, _url, body) => {
          postBody = body;
          return jsonResponse(201, {
            fleet_key_id: "fleet_key_001",
            key: "agt_test_raw_key_value_only_shown_once",
            created_at: Date.now(),
          });
        },
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          [
            "fleet-key", "create",
            "--workspace", WS_ID,
            "--fleet", FLEET_ID,
            "--name", "langgraph-bot",
            "--description", "external orchestration",
          ],
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        const text = out.read();
        expect(text).toContain("fleet_key_001");
        expect(text).toContain("agt_test_raw_key_value_only_shown_once");
        // The shown-once warning must be present in non-JSON mode.
        expect(text).toMatch(/shown once/i);

        // POST body shape contract: fleet_id + name + description.
        const parsed = JSON.parse(postBody ?? "{}") as {
          fleet_id?: string;
          name?: string;
          description?: string;
        };
        expect(parsed.fleet_id).toBe(FLEET_ID);
        expect(parsed.name).toBe("langgraph-bot");
        expect(parsed.description).toBe("external orchestration");

        expect(calls.map((c) => c.method)).toEqual(["POST"]);
      });
    });
  });

  test("`fleet-key list` GETs the workspace's external fleet keys and prints a table", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`GET /v1/workspaces/${WS_ID}/fleet-keys`]: () => jsonResponse(200, {
          items: [
            { fleet_key_id: "fleet_a", name: "langgraph-bot", description: "alpha", last_used_at: 1700000000000 },
            { fleet_key_id: "fleet_b", name: "crewai-bot",    description: "beta",  last_used_at: null },
          ],
        }),
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["fleet-key", "list", "--workspace", WS_ID],
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        const text = out.read();
        expect(text).toContain("langgraph-bot");
        expect(text).toContain("crewai-bot");
        expect(text).toContain("fleet_a");
        expect(text).toContain("fleet_b");
        expect(text).toContain("never");  // last_used_at: null renders as "never"
        expect(calls.map((c) => `${c.method} ${c.path}`)).toEqual([
          `GET /v1/workspaces/${WS_ID}/fleet-keys`,
        ]);
      });
    });
  });

  test("`fleet-key delete <id>` DELETEs the key and prints invalidation confirmation", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`DELETE /v1/workspaces/${WS_ID}/fleet-keys/01900000-0000-7000-8000-0000a6e7de7e`]:
          () => jsonResponse(204, {}),
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["fleet-key", "delete", "--workspace", WS_ID, "01900000-0000-7000-8000-0000a6e7de7e"],
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        expect(out.read()).toMatch(/01900000-0000-7000-8000-0000a6e7de7e.*invalidated/i);
        expect(calls.map((c) => `${c.method} ${c.path}`)).toEqual([
          `DELETE /v1/workspaces/${WS_ID}/fleet-keys/01900000-0000-7000-8000-0000a6e7de7e`,
        ]);
      });
    });
  });
});
