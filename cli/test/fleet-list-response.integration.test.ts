import { describe, expect, test } from "bun:test";
import { runCli } from "../src/cli.ts";
import { wsFleetsPath } from "../src/lib/api-paths.ts";
import { AGENTSFLEET_STATUS } from "../src/constants/fleet-status.ts";
import { FLEET_LIST_INVALID } from "../src/commands/fleet-read.ts";
import { bufferStream, cliEnv, withAuthedStateDir } from "./helpers-cli-state.ts";
import { jsonResponse, withMockApi } from "./helpers-mock-api.ts";

const WS_ID = "01900000-0000-7000-8000-000000b00b00";
const FLEET_ID = "01900000-0000-7000-8000-000000c0ffee";
const CURSOR = "opaque-next-page";
const ROW = {id: FLEET_ID, name: "check-failures", description: null,
  status: AGENTSFLEET_STATUS.ACTIVE, created_at: "1970-01-01T00:00:00Z",
  events_processed: "0", budget_used_nanos: "0"};
const PAGE = {items: [ROW], next_cursor: CURSOR, total: null, metadata: {served: true}};
const NO_FLEETS = "No fleets in this workspace.";
const FORMATS = [[], ["--json"]];

async function run(body: unknown, flags: string[] = []) {
  return withAuthedStateDir({workspaceId: WS_ID}, async () =>
    withMockApi({[`GET ${wsFleetsPath(WS_ID)}`]: () => jsonResponse(200, body)}, async (apiUrl, calls) => {
      const out = bufferStream();
      const err = bufferStream();
      const code = await runCli(["list", ...flags], {stdout: out.stream, stderr: err.stream,
        env: cliEnv({AGENTSFLEET_API_URL: apiUrl})});
      return {code, stdout: out.read(), stderr: err.read(), calls};
    }));
}

describe("fleet list validates the served page", () => {
  test.each([
    ["missing collection", {}],
    ["non-array collection", {...PAGE, items: {}}],
    ["null row", {...PAGE, items: [null]}],
    ["missing identity", {...PAGE, items: [{...ROW, id: undefined}]}],
    ["object name", {...PAGE, items: [{...ROW, name: {label: ROW.name}}]}],
    ["numeric status", {...PAGE, items: [{...ROW, status: 7}]}],
    ["numeric counter", {...PAGE, items: [{...ROW, events_processed: 7}]}],
    ["numeric money", {...PAGE, items: [{...ROW, budget_used_nanos: 7}]}],
    ["numeric creation time", {...PAGE, items: [{...ROW, created_at: 7}]}],
    ["invalid creation time", {...PAGE, items: [{...ROW, created_at: "yesterday"}]}],
    ["impossible creation time", {...PAGE, items: [{...ROW, created_at: "2026-13-01T00:00:00Z"}]}],
    ["missing cursor", {items: [ROW]}],
    ["invalid cursor", {...PAGE, next_cursor: {next: CURSOR}}],
  ])("refuses %s before text or JSON success", async (_name, body) => {
    for (const flags of FORMATS) {
      const result = await run(body, flags);
      expect(result.code).not.toBe(0);
      expect(result.stderr + result.stdout).toContain(FLEET_LIST_INVALID);
      expect(result.stderr + result.stdout).toContain("The fleet response did not match the supported API shape");
      expect(result.stderr + result.stdout).not.toMatch(/^\s+at /mu);
      expect(result.stdout).not.toContain(NO_FLEETS);
      expect(result.stdout).not.toContain("[object Object]");
      expect(result.calls).toHaveLength(1);
    }
  });

  test("preserves the complete valid JSON envelope and opaque cursor", async () => {
    const result = await run(PAGE, ["--json", "--limit", "2", "--starting-after", CURSOR]);
    expect(result.code).toBe(0);
    expect(JSON.parse(result.stdout)).toEqual(PAGE);
    expect(result.calls).toHaveLength(1);
    expect(result.calls[0]?.search).toBe(`?starting_after=${CURSOR}&limit=2`);
  });

  test("renders canonical fleet identity and a usable next-page hint", async () => {
    const result = await run(PAGE);
    expect(result.code).toBe(0);
    expect(result.stdout).toContain(ROW.name);
    expect(result.stdout).toContain(FLEET_ID.slice(0, 8));
    expect(result.stdout).toContain(`agentsfleet list --starting-after ${CURSOR}`);
  });

  test("an explicitly empty page is still a successful empty workspace", async () => {
    const result = await run({items: [], next_cursor: null});
    expect(result.code).toBe(0);
    expect(result.stdout).toContain(NO_FLEETS);
  });
});
