// `--log-level` validated a level and then governed nothing: this package
// emitted no records at all, so `debug` and `none` produced byte-identical
// output. A client that dials a server and carries a retry policy you cannot
// watch is the one you need at two in the morning.

import { describe, test, expect } from "bun:test";

import { runCli } from "../src/cli.ts";
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
