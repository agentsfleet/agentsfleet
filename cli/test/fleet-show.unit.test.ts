// `agentsfleet status` answered for the whole workspace and there was no way
// to ask about one fleet. `status` keeps that meaning — narrowing it would
// change a shipped command for every caller — and the one-fleet read is
// `fleet show <fleet_id>`, named the way every other single-resource read on
// this surface is named.

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

describe("one fleet has a one-fleet command", () => {
  test("fleet show reports that fleet and no other", async () => {
    const { code, text } = await run(["fleet", "show", WANTED]);
    expect(code).toBe(0);
    expect(text).toContain("reviewer");
    expect(text).not.toContain("responder");
  });

  test("status still reports every fleet in the workspace, and takes no identifier", async () => {
    const { code, text } = await run(["status"]);
    expect(code).toBe(0);
    expect(text).toContain("reviewer");
    expect(text).toContain("responder");
  });

  test("fleet show refuses a malformed identifier before a request is issued", async () => {
    // The unroutable API proves the refusal is client-side: a dial would
    // surface as a connection error rather than INVALID_ARGUMENT.
    const { code, text } = await run(["fleet", "show", "not-a-uuid", "--json"], UNROUTABLE);
    expect(code).not.toBe(0);
    expect(text).toContain("INVALID_ARGUMENT");
    expect(text).not.toContain("ECONNREFUSED");
  });

  test("status takes no identifier at all — the workspace view is its whole job", async () => {
    const { code, text } = await run(["status", WANTED]);
    expect(code).not.toBe(0);
    expect(text).toContain("Unexpected positional argument");
  });
});
