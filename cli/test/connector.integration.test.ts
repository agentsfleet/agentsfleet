import { describe, test, expect } from "bun:test";

import { runCli } from "../src/cli.ts";
import { saveCredentials, saveWorkspaces } from "../src/lib/state.ts";
import { bufferStream, withAuthedStateDir, withFreshStateDir } from "./helpers-cli-state.ts";
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
            team: "agentsfleet\u001b[31m-dev",
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
        expect(text).toContain("agentsfleet[31m-dev");
        expect(text).not.toContain("\u001b");
        expect(text).not.toContain("ignored");
        expect(calls.map((c) => `${c.method} ${c.path}`)).toEqual([
          `GET /v1/workspaces/${WS_ID}/connectors/slack`,
        ]);
      });
    });
  });

  test("`connector status --json` preserves raw backend strings for machines", async () => {
    await authedScope(async () => {
      const rawTeam = "agentsfleet\u001b[31m-dev";
      const routes: MockRoutes = {
        [`GET /v1/workspaces/${WS_ID}/connectors/slack`]: () =>
          jsonResponse(200, { status: "connected", team: rawTeam }),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["connector", "status", "slack", "--workspace", WS_ID, "--json"],
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );

        expect(code).toBe(0);
        expect(JSON.parse(out.read())).toEqual({ status: "connected", team: rawTeam });
        expect(err.read()).toBe("");
      });
    });
  });

  test("invalid connector provider fails before any API request", async () => {
    await authedScope(async () => {
      await withMockApi({}, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["connector", "status", "Slack/Bad", "--workspace", WS_ID],
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );

        expect(code).not.toBe(0);
        expect(`${out.read()}\n${err.read()}`).toContain("provider must be lowercase");
        expect(calls).toEqual([]);
      });
    });
  });

  test("connector list requires a workspace context before any API request", async () => {
    await withFreshStateDir(async () => {
      await saveCredentials({
        token: "header.payload.sig",
        saved_at: Date.now(),
        session_id: "sess_connector_no_workspace",
        api_url: null,
      });
      await saveWorkspaces({ current_workspace_id: null, items: [] });
      await withMockApi({}, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["connector", "list"],
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );

        expect(code).not.toBe(0);
        expect(`${out.read()}\n${err.read()}`).toContain("connector command requires --workspace");
        expect(calls).toEqual([]);
      });
    });
  });
});
