// The table reshape moved the HUMAN rendering of thirteen lists. Nothing that
// parses this CLI should have noticed.
//
// Every script, the acceptance suite and the dashboard's own tooling read
// `--json`, so a reshape that leaked into that register would break callers
// who never asked for a prettier table. The guard is per-register: the raw
// wire instant survives in JSON, and none of the text renderer's vocabulary
// — the AGO header, an age like `2h` — appears there at all.

import { describe, test, expect } from "bun:test";

import { runCli } from "../src/cli.ts";
import { bufferStream, withAuthedStateDir, cliEnv } from "./helpers-cli-state.ts";
import { withMockApi, jsonResponse, type MockRoutes } from "./helpers-mock-api.ts";

const WS_ID = "01900000-0000-7000-8000-00000067e210";
const FLEET_ID = "01900000-0000-7000-8000-0000000f1ee7";
const ENTRY_ID = "01900000-0000-7000-8000-00000011b111";
const CREATED_MS = 1700000000000;

// An age renders as digits plus one unit letter. If any of these reach a JSON
// payload, the text renderer has leaked into the machine register.
const AGE_SHAPED = /"\d+[smhdy]"/;

const routes: MockRoutes = {
  [`GET /v1/workspaces/${WS_ID}/secrets`]: () =>
    jsonResponse(200, { secrets: [{ name: "github", kind: "custom_secret", created_at: CREATED_MS }] }),
  [`GET /v1/workspaces/${WS_ID}/fleets`]: () =>
    jsonResponse(200, { items: [{ id: FLEET_ID, name: "reviewer", status: "active", created_at: CREATED_MS }] }),
  [`GET /v1/workspaces/${WS_ID}/fleet-libraries`]: () =>
    jsonResponse(200, { items: [{ id: ENTRY_ID, name: "reviewer", visibility: "tenant", created_at: CREATED_MS }] }),
  "GET /v1/api-keys": () =>
    jsonResponse(200, { items: [{ id: ENTRY_ID, key_name: "ci", active: true, created_at: CREATED_MS, last_used_at: CREATED_MS }] }),
  "GET /v1/workspaces": () =>
    jsonResponse(200, { items: [{ id: WS_ID, name: "main", created_at: CREATED_MS }], tenant_id: "t", total: null, next_cursor: null }),
};

const authedScope = <T>(fn: (stateDir: string) => Promise<T>): Promise<T> =>
  withAuthedStateDir({ workspaceId: WS_ID, sessionId: "sess_json_regression" }, fn);

const runJson = async (argv: ReadonlyArray<string>): Promise<string> => {
  let text = "";
  await authedScope(async () => {
    await withMockApi(routes, async (apiUrl) => {
      const out = bufferStream();
      const err = bufferStream();
      const code = await runCli([...argv, "--json"], {
        stdout: out.stream,
        stderr: err.stream,
        env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }),
      });
      expect(code).toBe(0);
      text = out.read();
    });
  });
  return text;
};

describe("the table reshape did not reach the machine register", () => {
  test.each([
    ["secret list", ["secret", "list"]],
    ["list", ["list"]],
    ["library", ["library"]],
    ["api-key list", ["api-key", "list"]],
  ])("%s --json keeps the raw wire instant and no rendered age", async (_label, argv) => {
    const text = await runJson(argv as ReadonlyArray<string>);

    // It is still JSON, and it still carries the instant the server sent.
    expect(() => JSON.parse(text)).not.toThrow();
    expect(text).toContain(String(CREATED_MS));

    // None of the text renderer's vocabulary crossed over.
    expect(text).not.toContain("AGO");
    expect(text).not.toMatch(AGE_SHAPED);
  });

  // `workspace list --json` answers from the LOCAL store rather than the read
  // it just made, so the server's instant is not what it echoes. The half that
  // still bites is the same: the renderer's vocabulary must not appear.
  test("workspace list --json answers from the local store, with no rendered age", async () => {
    const text = await runJson(["workspace", "list"]);
    expect(() => JSON.parse(text)).not.toThrow();
    expect(text).toContain(WS_ID);
    expect(text).not.toContain("AGO");
    expect(text).not.toMatch(AGE_SHAPED);
  });
});
