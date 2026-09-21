// `agentsfleet status` answered for the whole workspace and had no way to
// answer for one fleet, so the narrowest question needed the widest command.

import { describe, test, expect } from "bun:test";

import { runCli } from "../src/cli.ts";
import { bufferStream, withAuthedStateDir, cliEnv } from "./helpers-cli-state.ts";
import { withMockApi, jsonResponse, type MockRoutes } from "./helpers-mock-api.ts";

const WS_ID = "01900000-0000-7000-8000-00000067e210";
const WANTED = "01900000-0000-7000-8000-0000000f1ee7";
const OTHER = "01900000-0000-7000-8000-0000000f1ee8";
const UNROUTABLE = "http://127.0.0.1:9/";

const routes: MockRoutes = {
  [`GET /v1/workspaces/${WS_ID}/fleets`]: () =>
    jsonResponse(200, {
      items: [
        { id: WANTED, name: "reviewer", status: "active" },
        { id: OTHER, name: "responder", status: "stopped" },
      ],
    }),
  [`GET /v1/workspaces/${WS_ID}/fleets/${WANTED}`]: () =>
    jsonResponse(200, { id: WANTED, name: "reviewer", status: "active" }),
  [`GET /v1/workspaces/${WS_ID}/approvals`]: () => jsonResponse(200, { items: [] }),
};

const run = async (argv: ReadonlyArray<string>, apiOverride?: string) => {
  let text = "";
  let code = -1;
  await withAuthedStateDir({ workspaceId: WS_ID, sessionId: "sess_status_arg" }, async () => {
    await withMockApi(routes, async (apiUrl) => {
      const out = bufferStream();
      const err = bufferStream();
      code = await runCli([...argv], {
        stdout: out.stream,
        stderr: err.stream,
        env: cliEnv({ AGENTSFLEET_API_URL: apiOverride ?? apiUrl }),
      });
      text = `${out.read()}\n${err.read()}`;
    });
  });
  return { code, text };
};

describe("status reports one fleet or the whole workspace", () => {
  test("an identifier reports that fleet and no other", async () => {
    const { code, text } = await run(["status", WANTED]);
    expect(code).toBe(0);
    expect(text).toContain("reviewer");
    expect(text).not.toContain("responder");
  });

  test("bare status still reports every fleet in the workspace", async () => {
    const { code, text } = await run(["status"]);
    expect(code).toBe(0);
    expect(text).toContain("reviewer");
    expect(text).toContain("responder");
  });

  test("a malformed identifier is refused before a request is issued", async () => {
    // The unroutable API proves the refusal is client-side: a dial would
    // surface as a connection error rather than INVALID_ARGUMENT.
    const { code, text } = await run(["status", "not-a-uuid", "--json"], UNROUTABLE);
    expect(code).not.toBe(0);
    expect(text).toContain("INVALID_ARGUMENT");
    expect(text).not.toContain("ECONNREFUSED");
  });
});
