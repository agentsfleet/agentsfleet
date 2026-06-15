import { describe, test, expect } from "bun:test";

import { runCli } from "../src/cli.ts";
import { bufferStream, withAuthedStateDir } from "./helpers-cli-state.ts";
import { withMockApi, jsonResponse, type MockRoutes } from "./helpers-mock-api.ts";

const WS_ID = "01900000-0000-7000-8000-00000067e210";
const AGENTSFLEET_ID = "01900000-0000-7000-8000-0000007670f7";
const authedScope = <T>(fn: (stateDir: string) => Promise<T>): Promise<T> =>
  withAuthedStateDir({ workspaceId: WS_ID, sessionId: "sess_grant" }, fn);

describe("grant (integration grant) commands", () => {
  test("`grant list --agent <id>` GETs the grants for the agent and prints the table", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`GET /v1/workspaces/${WS_ID}/agents/${AGENTSFLEET_ID}/integration-grants`]:
          () => jsonResponse(200, {
            items: [
              { grant_id: "01900000-0000-7000-8000-000000067a01", service: "github", status: "approved",
                requested_at: 1700000000000, approved_at: 1700000000500 },
              { grant_id: "grant_2", service: "slack", status: "pending",
                requested_at: 1700000001000, approved_at: null },
            ],
          }),
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["grant", "list", "--agent", AGENTSFLEET_ID],
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        const text = out.read();
        expect(text).toContain("github");
        expect(text).toContain("slack");
        expect(text).toContain("01900000-0000-7000-8000-000000067a01");
        expect(text).toContain("grant_2");
        expect(text).toContain("approved");
        expect(text).toContain("pending");
        expect(calls.map((c) => `${c.method} ${c.path}`)).toEqual([
          `GET /v1/workspaces/${WS_ID}/agents/${AGENTSFLEET_ID}/integration-grants`,
        ]);
      });
    });
  });

  test("`grant delete --agent <id> <grant_id>` DELETEs the grant and prints the revocation note", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`DELETE /v1/workspaces/${WS_ID}/agents/${AGENTSFLEET_ID}/integration-grants/01900000-0000-7000-8000-000000067a01`]:
          () => jsonResponse(204, {}),
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["grant", "delete", "--agent", AGENTSFLEET_ID, "01900000-0000-7000-8000-000000067a01"],
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        const text = out.read();
        expect(text).toContain("01900000-0000-7000-8000-000000067a01");
        // The CLI's success line tells the operator the agent can no
        // longer use this integration. Server UZ-* codes are deliberately
        // not leaked into operator-facing output — UZ-GRANT-003 is what
        // the agent sees on retry, not what the operator gets here.
        expect(text).toContain("can no longer use this integration");
        expect(calls.map((c) => `${c.method} ${c.path}`)).toEqual([
          `DELETE /v1/workspaces/${WS_ID}/agents/${AGENTSFLEET_ID}/integration-grants/01900000-0000-7000-8000-000000067a01`,
        ]);
      });
    });
  });
});
