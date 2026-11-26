import { afterEach, beforeEach, expect, test } from "bun:test";
import { stripVTControlCharacters } from "node:util";
import { PlatformFixture, MODEL, RUNNER, RUNNER_DETAIL, RUNNER_ID, INSTANT } from "./helpers-platform.ts";

let fixture: PlatformFixture;
beforeEach(() => {fixture = new PlatformFixture();});
afterEach(() => fixture.dispose());

test("shows the default engine in the runner table while preserving the API JSON", async () => {
  const page = {items: [RUNNER, {...RUNNER, liveness: "offline"}], total: 2, next_cursor: null};
  fixture.respond = () => Response.json(page);
  const terminal = await fixture.run(["runners", "list"], "", true, false, {TERM: "xterm-256color", FORCE_COLOR: "2"});
  expect(terminal.code).toBe(0);
  expect(stripVTControlCharacters(terminal.stdout)).toMatch(/^ID\s+HOST\s+OWNER\s+STATE\s+CONNECTION/m);
  expect(terminal.stdout).toContain("\x1b[38;5;220mactive\x1b[0m");
  expect(terminal.stdout).toContain("\x1b[38;5;78monline\x1b[0m");
  expect(terminal.stdout).toContain("\x1b[38;5;210moffline\x1b[0m");
  expect(terminal.stdout).toContain("DEFAULT ENGINE");
  expect(terminal.stdout).toContain("Bubblewrap");
  const json = await fixture.run(["runners", "list", "--json"]);
  expect(json.code).toBe(0);
  expect(JSON.parse(json.stdout)).toEqual(page);
});

test("renders a runner's identity, policy and checks for an interactive terminal", async () => {
  fixture.respond = () => Response.json({...RUNNER_DETAIL, selftest: {
    all_ok: true, sandbox_tier: "landlock_full", network_policy: "deny_all_egress",
    checks: [{name: "bubblewrap", ok: true, detail: "Available"}],
  }});
  const result = await fixture.run(["runners", "show", RUNNER_ID], "", true);
  expect(result.code).toBe(0); expect(result.stdout).toContain("build-host");
  expect(result.stdout).toContain("Isolation tier"); expect(result.stdout).toContain("landlock_full");
  expect(result.stdout).toContain("Default engine"); expect(result.stdout).toContain("Bubblewrap");
  expect(result.stdout).toContain("bubblewrap: Available"); expect(result.stderr).toBe("");
});

test("renders each runner's processors, memory and free disk", async () => {
  const host = {cpu_count: 8, memory_bytes: 17_179_869_184, free_disk_bytes: 34_359_738_368};
  fixture.respond = () => Response.json({items: [{...RUNNER, host}, RUNNER], total: 2, next_cursor: null});
  const table = await fixture.run(["runners", "list"], "", true);
  expect(table.code).toBe(0);
  expect(stripVTControlCharacters(table.stdout)).toMatch(/CPUS\s+MEMORY\s+FREE DISK/u);
  expect(table.stdout).toContain("17.2 GB"); expect(table.stdout).toContain("34.4 GB");
  expect(table.stdout).toContain("Not reported");
  fixture.respond = () => Response.json({...RUNNER_DETAIL, host});
  const detail = await fixture.run(["runners", "show", RUNNER_ID], "", true);
  expect(detail.code).toBe(0);
  expect(stripVTControlCharacters(detail.stdout)).toMatch(/CPUs\s+·\s+8\n/u);
  expect(detail.stdout).toContain("17.2 GB"); expect(detail.stdout).toContain("34.4 GB");
});

test.each([[true, true], [true, false], [false, false]] as const)("keeps JSON and pipe output free of animation (json: %s, terminal: %s)", async (json, tty) => {
  fixture.respond = async () => {
    await Bun.sleep(460);
    return Response.json(RUNNER_DETAIL);
  };
  const result = await fixture.run(["runners", "show", RUNNER_ID, ...(json ? ["--json"] : [])], "", tty);
  expect(result.code).toBe(0); expect(JSON.parse(result.stdout).id).toBe(RUNNER_ID); expect(result.stderr).toBe("");
});

test("prints the inspected model as labelled fields", async () => {
  fixture.respond = () => Response.json({models: [MODEL], next_cursor: null});
  const model = await fixture.run(["models", "show", RUNNER_ID], "", true);
  expect(model.code).toBe(0); expect(model.stdout).toContain("Reasoning model"); expect(model.stdout).toContain("Context tokens");
});

test("cleans up the progress line before rendering a refused request", async () => {
  fixture.respond = async () => {
    await Bun.sleep(460);
    return Response.json({error_code: "UZ-AUTH-022", user_message: "Refused"}, {status: 403});
  };
  const result = await fixture.run(["runners", "show", RUNNER_ID], "", true);
  expect(result.code).not.toBe(0);
  expect(result.stderr).toContain("Waiting for agentsfleet…");
  expect(result.stderr.indexOf("Waiting for agentsfleet")).toBeLessThan(result.stderr.indexOf("UZ-AUTH-022"));
  expect(result.stdout).toBe("");
});

test.each([
  {patch: {}, expected: "Not reported"},
  {patch: {selftest_requested_at: INSTANT}, expected: "Pending"},
  {patch: {selftest_completed_at: INSTANT}, expected: "Passed"},
  {patch: {selftest_completed_at: INSTANT, selftest_requested_at: "2026-11-10T00:00:01Z"}, expected: "Pending"},
  {patch: {selftest_completed_at: "2026-11-10T00:00:00.100000001Z", selftest_requested_at: "2026-11-10T00:00:00.100000002Z"}, expected: "Pending"},
  {patch: {selftest_completed_at: "2026-11-10T00:00:00.1Z", selftest_requested_at: "2026-11-10T00:00:00.100Z"}, expected: "Passed"},
  {patch: {selftest_completed_at: null}, expected: "Stale (previous assignment)"},
  {patch: {selftest_completed_at: INSTANT, assigned_policy: {...RUNNER_DETAIL.assigned_policy, sandbox_tier: "container_nested"}}, expected: "Stale (previous assignment)"},
  {patch: {selftest_completed_at: INSTANT, assigned_policy: {...RUNNER_DETAIL.assigned_policy, network_policy: "allow_all"}}, expected: "Stale (previous assignment)"},
  {patch: {selftest_completed_at: INSTANT, selftest: {checks: [], all_ok: false, sandbox_tier: "landlock_full", network_policy: "deny_all_egress"}}, expected: "Failed"},
])("labels the recorded self-test as $expected", async ({patch, expected}) => {
  const report = {checks: [], all_ok: true, sandbox_tier: "landlock_full", network_policy: "deny_all_egress"};
  fixture.respond = () => Response.json({...RUNNER_DETAIL, selftest: expected === "Not reported" || (expected === "Pending" && !("selftest_completed_at" in patch)) ? null : report, ...patch});
  const result = await fixture.run(["runners", "show", RUNNER_ID], "", true);
  expect(result.code).toBe(0); expect(result.stdout).toContain(expected);
});

test("names each host's owner in the operator's runner table and detail", async () => {
  const tenant = "0199a801-1000-7000-8000-0000000000aa";
  fixture.respond = () => Response.json({items: [{...RUNNER, tenant_id: tenant}, RUNNER], total: 2, next_cursor: null});
  const table = await fixture.run(["runners", "list"], "", true);
  expect(table.code).toBe(0);
  expect(table.stdout).toContain("OWNER"); expect(table.stdout).toContain(tenant); expect(table.stdout).toContain("Platform");
  fixture.respond = () => Response.json({...RUNNER_DETAIL, tenant_id: tenant});
  const detail = await fixture.run(["runners", "show", RUNNER_ID], "", true);
  expect(detail.code).toBe(0); expect(detail.stdout).toContain("Owner"); expect(detail.stdout).toContain(tenant);
});
