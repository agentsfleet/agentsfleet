import { describe, test, expect } from "bun:test";

import { runCli } from "../src/cli.ts";
import { bufferStream, withAuthedStateDir, cliEnv } from "./helpers-cli-state.ts";
import { withMockApi, jsonResponse, type MockRoutes } from "./helpers-mock-api.ts";

const WS_ID = "01900000-0000-7000-8000-00000067e210";
const FLEET_ID = "01900000-0000-7000-8000-0000007670f7";
const authedScope = <T>(fn: (stateDir: string) => Promise<T>): Promise<T> =>
  withAuthedStateDir({ workspaceId: WS_ID, sessionId: "sess_grant" }, fn);

describe("grant (integration grant) commands", () => {
  test("`grant list --fleet <id>` GETs the grants for the fleet and prints the table", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`GET /v1/workspaces/${WS_ID}/fleets/${FLEET_ID}/integration-grants`]:
          () => jsonResponse(200, {
            items: [
              { id: "01900000-0000-7000-8000-000000067a01", kind: "integration", name: "github", status: "approved",
                approved_at: "2023-11-14T22:13:20.5Z" },
              { id: "grant_2", kind: "secret", name: "sentry", status: "pending", approved_at: null },
            ],
          }),
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["fleet", "grants", "list", "--fleet", FLEET_ID],
          { stdout: out.stream, stderr: err.stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }) },
        );
        expect(code).toBe(0);
        const text = out.read();
        expect(text).toContain("github");
        expect(text).toContain("integration");
        expect(text).toContain("sentry");
        expect(text).toContain("secret");
        expect(text).toContain("01900000-0000-7000-8000-000000067a01");
        expect(text).toContain("grant_2");
        expect(text).toContain("approved");
        expect(text).toContain("pending");
        expect(calls.map((c) => `${c.method} ${c.path}`)).toEqual([
          `GET /v1/workspaces/${WS_ID}/fleets/${FLEET_ID}/integration-grants`,
        ]);
      });
    });
  });

  test("grant list shows a host grant in the table and in JSON", async () => {
    await authedScope(async () => {
      const host = {id: "grant_host", kind: "host", name: "docs.vendor.example", status: "approved"};
      const secret = {...host, id: "grant_secret", kind: "secret"};
      const path = `GET /v1/workspaces/${WS_ID}/fleets/${FLEET_ID}/integration-grants`;
      await withMockApi({[path]: () => jsonResponse(200, {items: [host, secret]})}, async apiUrl => {
        for (const flags of [[], ["--json"]]) {
          const out = bufferStream();
          const code = await runCli(["fleet", "grants", "list", "--fleet", FLEET_ID, ...flags], {
            stdout: out.stream, stderr: bufferStream().stream, env: cliEnv({AGENTSFLEET_API_URL: apiUrl}),
          });
          expect(code).toBe(0);
          if (flags.length) {
            expect(JSON.parse(out.read()).items).toEqual([host, secret]);
          } else {
            const rows = out.read().split("\n").filter(row => row.includes(host.name));
            expect(rows).toHaveLength(2);
            expect(rows[0]).toContain("host");
            expect(rows[1]).toContain("secret");
          }
        }
      });
    });
  });

  test("`grant list` prints a malformed approval instant as the empty cell instead of crashing", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`GET /v1/workspaces/${WS_ID}/fleets/${FLEET_ID}/integration-grants`]:
          () => jsonResponse(200, {
            items: [
              { id: "grant_bad", kind: "integration", name: "github", status: "approved",
                approved_at: "not-an-instant" },
            ],
          }),
      };
      await withMockApi(routes, async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["fleet", "grants", "list", "--fleet", FLEET_ID],
          { stdout: out.stream, stderr: err.stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }) },
        );
        expect(code).toBe(0);
        const text = out.read();
        expect(text).toContain("grant_bad");
        expect(text).toContain("—");
        expect(text).not.toContain("not-an-instant");
      });
    });
  });

  test("`grant delete --fleet <id> <grant_id>` DELETEs the grant and prints the revocation note", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`DELETE /v1/workspaces/${WS_ID}/fleets/${FLEET_ID}/integration-grants/01900000-0000-7000-8000-000000067a01`]:
          () => jsonResponse(204, {}),
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["fleet", "grants", "revoke", "--fleet", FLEET_ID, "01900000-0000-7000-8000-000000067a01"],
          { stdout: out.stream, stderr: err.stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }) },
        );
        expect(code).toBe(0);
        const text = out.read();
        expect(text).toContain("01900000-0000-7000-8000-000000067a01");
        expect(text).toContain("Future runs lose the access provided by this grant");
        expect(calls.map((c) => `${c.method} ${c.path}`)).toEqual([
          `DELETE /v1/workspaces/${WS_ID}/fleets/${FLEET_ID}/integration-grants/01900000-0000-7000-8000-000000067a01`,
        ]);
      });
    });
  });
});
