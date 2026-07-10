import { describe, test, expect } from "bun:test";

import { runCli } from "../src/cli.ts";
import { bufferStream, withAuthedStateDir } from "./helpers-cli-state.ts";
import { withMockApi, jsonResponse, type MockRoutes } from "./helpers-mock-api.ts";

const WS_ID = "01900000-0000-7000-8000-000000c011ec";
const authedScope = <T>(fn: () => Promise<T>): Promise<T> =>
  withAuthedStateDir({ workspaceId: WS_ID, sessionId: "sess_connector" }, fn);

describe("connector commands", () => {
  test("`connector list` prints configured and connected state", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`GET /v1/workspaces/${WS_ID}/connectors`]: () =>
          jsonResponse(200, [
            {
              id: "slack",
              archetype: "oauth2",
              display_name: "Slack",
              configured: false,
              connected: false,
            },
            {
              id: "github",
              archetype: "app_install",
              display_name: "GitHub",
              configured: true,
              connected: true,
            },
          ]),
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["connector", "list", "--workspace", WS_ID],
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );

        expect(code).toBe(0);
        const text = out.read();
        expect(text).toContain("slack");
        expect(text).toContain("admin setup required");
        expect(text).toContain("github");
        expect(text).toContain("configured");
        expect(text).toContain("connected");
        expect(calls.map((c) => `${c.method} ${c.path}`)).toEqual([
          `GET /v1/workspaces/${WS_ID}/connectors`,
        ]);
      });
    });
  });

  test("`connector status <provider>` prints primitive status fields", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`GET /v1/workspaces/${WS_ID}/connectors/slack`]: () =>
          jsonResponse(200, {
            status: "connected",
            team: "agentsfleet-dev",
            nested: { ignored: true },
          }),
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["connector", "status", "slack", "--workspace", WS_ID],
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );

        expect(code).toBe(0);
        const text = out.read();
        expect(text).toContain("provider");
        expect(text).toContain("slack");
        expect(text).toContain("status");
        expect(text).toContain("connected");
        expect(text).toContain("agentsfleet-dev");
        expect(text).not.toContain("ignored");
        expect(calls.map((c) => `${c.method} ${c.path}`)).toEqual([
          `GET /v1/workspaces/${WS_ID}/connectors/slack`,
        ]);
      });
    });
  });
});
