import { describe, test, expect } from "bun:test";

import { runCli } from "../src/cli.js";
import { bufferStream, withAuthedStateDir } from "./helpers-cli-state.js";
import { withMockApi, jsonResponse } from "./helpers-mock-api.js";

const WS_ID = "ws_grant_test";
const ZOMBIE_ID = "zmb_grant_test";
const authedScope = (fn) => withAuthedStateDir({ workspaceId: WS_ID, sessionId: "sess_grant" }, fn);

describe("grant (integration grant) commands", () => {
  test("`grant list --zombie <id>` GETs the grants for the zombie and prints the table", async () => {
    await authedScope(async () => {
      const routes = {
        [`GET /v1/workspaces/${WS_ID}/zombies/${ZOMBIE_ID}/integration-grants`]:
          () => jsonResponse(200, {
            items: [
              { grant_id: "grant_1", service: "github", status: "approved",
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
          ["grant", "list", "--zombie", ZOMBIE_ID],
          { stdout: out.stream, stderr: err.stream, env: { ZOMBIE_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        const text = out.read();
        expect(text).toContain("github");
        expect(text).toContain("slack");
        expect(text).toContain("grant_1");
        expect(text).toContain("grant_2");
        expect(text).toContain("approved");
        expect(text).toContain("pending");
        expect(calls.map((c) => `${c.method} ${c.path}`)).toEqual([
          `GET /v1/workspaces/${WS_ID}/zombies/${ZOMBIE_ID}/integration-grants`,
        ]);
      });
    });
  });

  test("`grant delete --zombie <id> <grant_id>` DELETEs the grant and prints the revocation note", async () => {
    await authedScope(async () => {
      const routes = {
        [`DELETE /v1/workspaces/${WS_ID}/zombies/${ZOMBIE_ID}/integration-grants/grant_1`]:
          () => jsonResponse(204, {}),
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["grant", "delete", "--zombie", ZOMBIE_ID, "grant_1"],
          { stdout: out.stream, stderr: err.stream, env: { ZOMBIE_API_URL: apiUrl } },
        );
        expect(code).toBe(0);
        const text = out.read();
        expect(text).toContain("grant_1");
        // The CLI's success line names UZ-GRANT-003 to set expectations for
        // what the zombie will see on its next execute attempt.
        expect(text).toContain("UZ-GRANT-003");
        expect(calls.map((c) => `${c.method} ${c.path}`)).toEqual([
          `DELETE /v1/workspaces/${WS_ID}/zombies/${ZOMBIE_ID}/integration-grants/grant_1`,
        ]);
      });
    });
  });
});
