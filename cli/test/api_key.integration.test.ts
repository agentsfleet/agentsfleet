import { describe, test, expect } from "bun:test";

import { runCli } from "../src/cli.ts";
import { bufferStream, withAuthedStateDir, cliEnv } from "./helpers-cli-state.ts";
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
          { stdout: out.stream, stderr: err.stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }) },
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

  const SECOND_KEY_ID = "01900000-0000-7000-8000-000000a91e91";
  const WALK_CURSOR = "cur_page_boundary";
  const walkKeyRow = (id: string, name: string) => ({
    id,
    key_name: name,
    active: true,
    created_at: 1700000000000,
    last_used_at: null,
    revoked_at: null,
  });
  // Two server pages split by a cursor boundary; the walk must cross it.
  const twoPageWalkRoutes = (expectedSort: string): MockRoutes => ({
    "GET /v1/api-keys": (_req, url) => {
      expect(url.searchParams.get("sort")).toBe(expectedSort);
      expect(url.searchParams.has("page")).toBe(false);
      expect(url.searchParams.has("page_size")).toBe(false);
      if (url.searchParams.get("starting_after") === null) {
        return jsonResponse(200, {
          items: [walkKeyRow(KEY_ID, "alpha-runner")],
          total: 2,
          next_cursor: WALK_CURSOR,
        });
      }
      expect(url.searchParams.get("starting_after")).toBe(WALK_CURSOR);
      return jsonResponse(200, {
        items: [walkKeyRow(SECOND_KEY_ID, "beta-runner")],
        total: 2,
        next_cursor: null,
      });
    },
  });

  test("test_api_key_list_returns_every_key", async () => {
    await authedScope(async () => {
      await withMockApi(twoPageWalkRoutes("-created_at"), async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["api-key", "list"],
          { stdout: out.stream, stderr: err.stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }) },
        );

        expect(code).toBe(0);
        const text = out.read();
        expect(text).toContain("alpha-runner");
        expect(text).toContain("beta-runner");
        expect(text).toContain("never");
        expect(calls.map((c) => `${c.method} ${c.path}`)).toEqual([
          "GET /v1/api-keys",
          "GET /v1/api-keys",
        ]);
      });
    });
  });

  test("test_api_key_list_sort_orders_complete_set", async () => {
    await authedScope(async () => {
      // The route asserts the sort rides EVERY read; the output asserts the
      // concatenated pages read as one ordered set, not per-read islands.
      await withMockApi(twoPageWalkRoutes("key_name"), async (apiUrl) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["api-key", "list", "--sort", "key_name"],
          { stdout: out.stream, stderr: err.stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }) },
        );

        expect(code).toBe(0);
        const text = out.read();
        expect(text.indexOf("alpha-runner")).toBeGreaterThanOrEqual(0);
        expect(text.indexOf("alpha-runner")).toBeLessThan(text.indexOf("beta-runner"));
      });
    });
  });

  test("test_api_key_list_json_mode_is_complete", async () => {
    await authedScope(async () => {
      await withMockApi(twoPageWalkRoutes("-created_at"), async (apiUrl, calls) => {
        const out = bufferStream();
        const err = bufferStream();
        const code = await runCli(
          ["--json", "api-key", "list"],
          { stdout: out.stream, stderr: err.stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }) },
        );

        expect(code).toBe(0);
        const printed = JSON.parse(out.read()) as {
          items: Array<{ id: string }>;
          total: number;
          next_cursor: string | null;
        };
        expect(printed.items.map((item) => item.id)).toEqual([KEY_ID, SECOND_KEY_ID]);
        expect(printed.total).toBe(2);
        expect(printed.next_cursor).toBeNull();
        expect(calls.length).toBe(2);
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
          { stdout: revokeOut.stream, stderr: revokeErr.stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }) },
        );
        const deleteOut = bufferStream();
        const deleteErr = bufferStream();
        const deleteCode = await runCli(
          ["api-key", "delete", KEY_ID],
          { stdout: deleteOut.stream, stderr: deleteErr.stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }) },
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
            env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }),
          });

          expect(code).not.toBe(0);
          expect(`${out.read()}\n${err.read()}`).toMatch(/required|integer|uuidv7|one of|name|≤ 100/i);
          expect(calls).toEqual([]);
        });
      }
    });
  });
});
