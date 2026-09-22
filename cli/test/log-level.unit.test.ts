// `--log-level` validated a level and then governed nothing: this package
// emitted no records at all, so `debug` and `none` produced byte-identical
// output. A client that dials a server and carries a retry policy you cannot
// watch is the one you need at two in the morning.

import { describe, test, expect } from "bun:test";

import { runCli } from "../src/cli.ts";
import { endpointOf } from "../src/services/http-client.ts";
import { bufferStream, withAuthedStateDir, cliEnv } from "./helpers-cli-state.ts";
import { withMockApi, jsonResponse, type MockRoutes } from "./helpers-mock-api.ts";

const WS_ID = "01900000-0000-7000-8000-00000067e210";
const FLEET_ID = "01900000-0000-7000-8000-0000000f1ee7";
const TRACE_MARKER = "http.attempt";

const routes: MockRoutes = {
  [`GET /v1/workspaces/${WS_ID}/fleets`]: () =>
    jsonResponse(200, { items: [{ id: FLEET_ID, name: "reviewer", status: "active" }] }),
};

const runAt = async (level: ReadonlyArray<string>): Promise<string> => {
  let combined = "";
  await withAuthedStateDir({ workspaceId: WS_ID, sessionId: "sess_log_level" }, async () => {
    await withMockApi(routes, async (apiUrl) => {
      const out = bufferStream();
      const err = bufferStream();
      const code = await runCli([...level, "list"], {
        stdout: out.stream,
        stderr: err.stream,
        env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }),
      });
      expect(code).toBe(0);
      combined = `${out.read()}\n${err.read()}`;
    });
  });
  return combined;
};

describe("the level governs records that exist", () => {
  test("debug reports the request, its attempt and how long it took", async () => {
    const text = await runAt(["--log-level", "debug"]);
    expect(text).toContain(TRACE_MARKER);
    expect(text).toContain("method=GET");
    expect(text).toContain("attempt=1");
    expect(text).toContain("duration_ms=");
  });

  test("none emits nothing, and so does the default", async () => {
    expect(await runAt(["--log-level", "none"])).not.toContain(TRACE_MARKER);
    expect(await runAt([])).not.toContain(TRACE_MARKER);
  });

  test("the endpoint is named, the fleet being looked at is not", async () => {
    const text = await runAt(["--log-level", "debug"]);
    expect(text).toContain("/v1/workspaces/{id}/fleets");
    // The record must not say WHICH workspace someone was reading.
    const traceLines = text.split("\n").filter((l) => l.includes(TRACE_MARKER));
    expect(traceLines.length).toBeGreaterThan(0);
    for (const line of traceLines) expect(line).not.toContain(WS_ID);
  });

  test("no record carries the bearer token", async () => {
    const text = await runAt(["--log-level", "debug"]);
    for (const line of text.split("\n").filter((l) => l.includes(TRACE_MARKER))) {
      expect(line.toLowerCase()).not.toContain("authorization");
      expect(line.toLowerCase()).not.toContain("bearer");
    }
  });

  test("a level we do not offer is still refused, naming the nine we do", async () => {
    const out = bufferStream();
    const err = bufferStream();
    const code = await runCli(["--log-level", "bogus", "list"], {
      stdout: out.stream,
      stderr: err.stream,
      env: cliEnv({}),
    });
    expect(code).not.toBe(0);
    for (const level of ["all", "trace", "debug", "info", "warn", "error", "fatal", "none"])
      expect(err.read()).toContain(level);
  });
});

// The redaction boundary itself, asserted directly rather than only through
// the paths today's commands happen to build. A record names the route
// somebody called; which row they were looking at is theirs.
describe("the endpoint a record names carries no identifier", () => {
  const U = "01900000-0000-7000-8000-00000067e210";
  const V = "01900000-0000-7000-8000-0000000f1ee7";

  test.each([
    ["a mid-path identifier", `/v1/workspaces/${U}/fleets`, "/v1/workspaces/{id}/fleets"],
    ["a trailing identifier", `/v1/fleets/${U}`, "/v1/fleets/{id}"],
    ["a trailing identifier before a query", `/v1/fleets/${U}?expand=runs`, "/v1/fleets/{id}?expand={value}"],
    ["a collection query", `/v1/workspaces/${U}/approvals?limit=50`, "/v1/workspaces/{id}/approvals?limit={value}"],
    ["two identifiers", `/v1/workspaces/${U}/library-entries/${V}`, "/v1/workspaces/{id}/library-entries/{id}"],
    ["two identifiers before a query", `/v1/workspaces/${U}/fleets/${V}?tail=1`, "/v1/workspaces/{id}/fleets/{id}?tail={value}"],
    // A cursor is a row identifier that does not look like one. The memory
    // cursor carries the memory key, so keeping the value would name the row
    // through the one part of the path nobody reads as an identifier.
    ["an opaque cursor", "/v1/memories?starting_after=bWVtb3J5OnNlY3JldC1rZXk", "/v1/memories?starting_after={value}"],
    ["several parameters", `/v1/workspaces/${U}/approvals?limit=50&starting_after=abc`, "/v1/workspaces/{id}/approvals?limit={value}&starting_after={value}"],
    ["a bare flag carrying no value", "/v1/fleets?all", "/v1/fleets?all"],
  ])("%s is replaced", (_label, path, expected) => {
    expect(endpointOf(path as string)).toBe(expected as string);
  });

  test("a parameter name survives, so a reader still sees which were sent", () => {
    const rendered = endpointOf("/v1/memories?starting_after=bWVtb3J5OnNlY3JldA&limit=25");
    expect(rendered).toContain("starting_after=");
    expect(rendered).toContain("limit=");
    expect(rendered).not.toContain("bWVtb3J5OnNlY3JldA");
    expect(rendered).not.toContain("25");
  });

  test("no rendered endpoint contains a UUID, whatever follows it", () => {
    const UUID_ANYWHERE = /[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}/u;
    for (const path of [
      `/v1/fleets/${U}`,
      `/v1/fleets/${U}?expand=runs`,
      `/v1/workspaces/${U}/library-entries/${V}`,
      `/v1/workspaces/${U}/library-entries/${V}?force=1`,
    ])
      expect(UUID_ANYWHERE.test(endpointOf(path))).toBe(false);
  });

  test("a path with nothing to redact is returned byte-identical", () => {
    expect(endpointOf("/v1/tenants/me/models")).toBe("/v1/tenants/me/models");
  });
});
