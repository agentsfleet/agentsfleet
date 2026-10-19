import { describe, expect, test } from "bun:test";
import { runCli } from "../src/cli.ts";
import { bufferStream, cliEnv, withAuthedStateDir } from "./helpers-cli-state.ts";
import { jsonResponse, withMockApi } from "./helpers-mock-api.ts";

const WS = "01900000-0000-7000-8000-000000b00b00";
const FLEET = "01900000-0000-7000-8000-000000c0ffee";
const SCHEDULE = "01900000-0000-7000-8000-000000bada55";
const PATH = `/v1/workspaces/${WS}/fleets/${FLEET}/schedules`;
const NEXT_RUN = "2026-11-01T03:30:00Z";
const ROW = {schedule_id: SCHEDULE, fleet_id: FLEET, cron: "0 9 * * *", timezone: "Asia/Kolkata",
  message: "summarize", status: "active", sync: "synced", description: "At 9:00 AM (Asia/Kolkata)",
  run_state: "scheduled", next_run_at: NEXT_RUN, created_at: "2026-10-01T00:00:00Z", updated_at: "2026-10-01T00:00:00Z"};
const INVALID = "SCHEDULE_RESPONSE_INVALID";
const VERBS = ["list", "show", "create", "update", "sync"] as const;
type Verb = typeof VERBS[number];

async function run(verb: Verb, body: unknown, json = false) {
  const method = verb === "create" || verb === "sync" ? "POST" : verb === "update" ? "PATCH" : "GET";
  const path = `${PATH}${verb === "list" || verb === "create" ? "" : `/${SCHEDULE}`}${verb === "sync" ? "/sync" : ""}`;
  const args = ["schedule", verb, FLEET, ...(verb === "list" || verb === "create" ? [] : [SCHEDULE]),
    ...(verb === "create" ? ["--cron", ROW.cron, "--message", ROW.message] : []),
    ...(verb === "update" ? ["--message", ROW.message] : []), ...(json ? ["--json"] : [])];
  return withAuthedStateDir({workspaceId: WS}, () => withMockApi({[`${method} ${path}`]: () => jsonResponse(200, body)}, async (apiUrl, calls) => {
    const out = bufferStream(); const err = bufferStream(); out.stream.isTTY = true;
    const code = await runCli(args, {stdout: out.stream, stderr: err.stream, env: cliEnv({AGENTSFLEET_API_URL: apiUrl})});
    return {code, stdout: out.read(), stderr: err.read(), calls};
  }));
}

describe("schedule responses are checked before rendering", () => {
  test.each([
    ["missing collection", {}], ["non-array collection", {schedules: {}}], ["null row", {schedules: [null]}],
    ["missing summary", {schedules: [{...ROW, description: undefined}]}],
    ["empty summary", {schedules: [{...ROW, description: ""}]}],
    ["numeric state", {schedules: [{...ROW, run_state: 7}]}],
    ["missing next run", {schedules: [{...ROW, next_run_at: undefined}]}],
    ["numeric next run", {schedules: [{...ROW, next_run_at: 7}]}],
    ["impossible next run", {schedules: [{...ROW, next_run_at: "2026-13-01T00:00:00Z"}]}],
    ["object message", {schedules: [{...ROW, message: {text: "secret"}}]}],
  ])("refuses %s in text and raw JSON", async (_name, body) => {
    for (const json of [false, true]) {
      const result = await run("list", body, json);
      expect(result.code).not.toBe(0);
      expect(result.stdout + result.stderr).toContain(INVALID);
      expect(result.stdout).not.toContain("No schedules");
      expect(result.stdout + result.stderr).not.toContain("secret");
      expect(result.calls).toHaveLength(1);
    }
  });

  test.each(VERBS)("%s refuses malformed schedule success", async (verb) => {
    const result = await run(verb, verb === "list" ? {schedules: [{}]} : {}, true);
    expect(result.code).not.toBe(0);
    expect(result.stdout + result.stderr).toContain(INVALID);
    expect(result.calls).toHaveLength(1);
  });

  test.each(VERBS)("%s retains extra server fields in valid raw JSON", async (verb) => {
    const row = {...ROW, source: "api", once: true, etag: "opaque", override: {message: "wake"}};
    const body = verb === "list" ? {schedules: [row], metadata: {served: true}} : row;
    const result = await run(verb, body, true);
    expect(result.code).toBe(0);
    expect(JSON.parse(result.stdout)).toEqual(body);
    expect(result.calls).toHaveLength(1);
  });

  test.each([
    ["scheduled", NEXT_RUN, "Scheduled", true], ["scheduled", null, "Next run unavailable", false],
    ["paused", NEXT_RUN, "Paused", false], ["activation_pending", NEXT_RUN, "Activation pending", false],
    ["next_run_unavailable", NEXT_RUN, "Next run unavailable", false], ["future_state", NEXT_RUN, "Status unavailable", false],
  ] as const)("renders server %s without promising a stale next run", async (run_state, next_run_at, label, showTime) => {
    for (const verb of ["list", "show"] as const) {
      const row = {...ROW, run_state, next_run_at};
      const result = await run(verb, verb === "list" ? {schedules: [row]} : row);
      expect(result.code).toBe(0);
      expect(result.stdout).toContain(ROW.description);
      expect(result.stdout).toContain(label);
      expect(result.stdout.includes(NEXT_RUN)).toBe(showTime);
      expect(result.stdout).not.toContain(ROW.cron);
    }
  });

  test("an explicitly empty list has a clear empty state", async () => {
    const result = await run("list", {schedules: []});
    expect(result.code).toBe(0);
    expect(result.stdout).toContain("No schedules for this Fleet.");
  });
});
