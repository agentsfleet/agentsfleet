import { describe, test, expect } from "bun:test";

import { runCli } from "../src/cli.ts";
import { bufferStream, withAuthedStateDir } from "./helpers-cli-state.ts";
import { withMockApi, jsonResponse, type MockRoutes } from "./helpers-mock-api.ts";

const WS_ID = "01900000-0000-7000-8000-000000a91eaf";
const KEY_ID = "01900000-0000-7000-8000-000000a91e90";
const RAW_KEY = "agt_t_test_raw_key_value_only_shown_once";
const authedScope = <T>(fn: () => Promise<T>): Promise<T> =>
  withAuthedStateDir({ workspaceId: WS_ID, sessionId: "sess_api_key" }, fn);

describe("api-key commands", () => {
  test("`api-key create` POSTs key_name and prints the raw key exactly once", async () => {
    await authedScope(async () => {
      let postBody: string | null = null;
      const routes: MockRoutes = {
        "POST /v1/api-keys": async (_req, _url, body) => {
          postBody = body;
          return jsonResponse(201, {
            id: KEY_ID,
            key_name: "ci-runner",
            key: RAW_KEY,
            created_at: 1700000000000,
          });
        },
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["api-key", "create", "--name", "ci-runner", "--description", "build automation"],
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );

        expect(code).toBe(0);
        const text = out.read();
        expect(text).toContain(KEY_ID);
        expect((text.match(new RegExp(RAW_KEY, "g")) ?? []).length).toBe(1);
        expect(text).toMatch(/shown once/i);
        expect(JSON.parse(postBody ?? "{}")).toEqual({
          key_name: "ci-runner",
          description: "build automation",
        });
        expect(calls.map((c) => `${c.method} ${c.path}`)).toEqual(["POST /v1/api-keys"]);
      });
    });
  });

  test("`api-key list` forwards pagination and renders last-used null as never", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        "GET /v1/api-keys": (_req, url) => {
          expect(url.searchParams.get("page")).toBe("2");
          expect(url.searchParams.get("page_size")).toBe("50");
          expect(url.searchParams.get("sort")).toBe("key_name");
          return jsonResponse(200, {
            items: [
              {
                id: KEY_ID,
                key_name: "ci-runner",
                active: true,
                created_at: 1700000000000,
                last_used_at: null,
                revoked_at: null,
              },
            ],
            total: 1,
            page: 2,
            page_size: 50,
          });
        },
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["api-key", "list", "--page", "2", "--page-size", "50", "--sort", "key_name"],
          { stdout: out.stream, stderr: err.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );

        expect(code).toBe(0);
        const text = out.read();
        expect(text).toContain("ci-runner");
        expect(text).toContain("active");
        expect(text).toContain("never");
        expect(calls.map((c) => `${c.method} ${c.path}`)).toEqual(["GET /v1/api-keys"]);
      });
    });
  });

  test("`api-key revoke` PATCHes active=false and `delete` deletes the revoked key", async () => {
    await authedScope(async () => {
      const routes: MockRoutes = {
        [`PATCH /v1/api-keys/${KEY_ID}`]: async (_req, _url, body) => {
          expect(JSON.parse(body ?? "{}")).toEqual({ active: false });
          return jsonResponse(200, { id: KEY_ID, active: false, revoked_at: 1700000001000 });
        },
        [`DELETE /v1/api-keys/${KEY_ID}`]: () => jsonResponse(204, {}),
      };
      await withMockApi(routes, async (apiUrl, calls) => {
        const revokeOut = bufferStream();
        const revokeErr = bufferStream();
        const revokeCode = await runCli(
          ["api-key", "revoke", KEY_ID],
          { stdout: revokeOut.stream, stderr: revokeErr.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );
        const deleteOut = bufferStream();
        const deleteErr = bufferStream();
        const deleteCode = await runCli(
          ["api-key", "delete", KEY_ID],
          { stdout: deleteOut.stream, stderr: deleteErr.stream, env: { AGENTSFLEET_API_URL: apiUrl } },
        );

        expect(revokeCode).toBe(0);
        expect(deleteCode).toBe(0);
        expect(revokeOut.read()).toContain("can no longer authenticate");
        expect(deleteOut.read()).toContain("deleted");
        expect(calls.map((c) => `${c.method} ${c.path}`)).toEqual([
          `PATCH /v1/api-keys/${KEY_ID}`,
          `DELETE /v1/api-keys/${KEY_ID}`,
        ]);
      });
    });
  });

  test("invalid api-key arguments fail before any API request", async () => {
    await authedScope(async () => {
      const invalidCases: ReadonlyArray<ReadonlyArray<string>> = [
        ["api-key", "create"],
        ["api-key", "list", "--page", "abc"],
        ["api-key", "list", "--page-size", "101"],
        ["api-key", "list", "--sort", "name"],
        ["api-key", "revoke", "not-a-uuid"],
      ];
      for (const argv of invalidCases) {
        await withMockApi({}, async (apiUrl, calls) => {
          const out = bufferStream();
          const err = bufferStream();
          const code = await runCli(argv, {
            stdout: out.stream,
            stderr: err.stream,
            env: { AGENTSFLEET_API_URL: apiUrl },
          });

          expect(code).not.toBe(0);
          expect(`${out.read()}\n${err.read()}`).toMatch(/required|integer|uuidv7|one of|name|≤ 100/i);
          expect(calls).toEqual([]);
        });
      }
    });
  });
});
