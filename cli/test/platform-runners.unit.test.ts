import { afterEach, beforeEach, expect, test } from "bun:test";
import { existsSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { PlatformFixture, POLICY, RUNNER, RUNNER_DETAIL, RUNNER_ID, WORKSPACE_ID } from "./helpers-platform.ts";

let fixture: PlatformFixture;
beforeEach(() => {fixture = new PlatformFixture();});
afterEach(() => fixture.dispose());

test("lists an operator page with the API's paging names", async () => {
  fixture.respond = () => Response.json({items: [RUNNER], total: null, next_cursor: RUNNER_ID});
  const result = await fixture.run(["runners", "list", "--limit", "100", "--starting-after", RUNNER_ID, "--json"]);
  expect(result.code).toBe(0); expect(JSON.parse(result.stdout).items[0].host_id).toBe("build-host");
  expect(fixture.calls[0]?.url.pathname).toBe("/v1/admin/runners");
  expect(fixture.calls[0]?.url.search).toBe(`?limit=100&starting_after=${RUNNER_ID}`);
});

test.each(["0", "101", "1.5", "many"])("refuses an unsupported runner page size %s before HTTP", async (limit) => {
  const result = await fixture.run(["runners", "list", "--limit", limit, "--json"]);
  expect(result.code).toBe(4); expect(JSON.parse(result.stderr).error.code).toBe("INVALID_ARGUMENT");
  expect(fixture.calls).toEqual([]);
});

test("reads runner policy and counters without a workspace context", async () => {
  fixture.respond = () => Response.json(RUNNER_DETAIL);
  const result = await fixture.run(["runners", "show", RUNNER_ID, "--json"]);
  expect(result.code).toBe(0); expect(JSON.parse(result.stdout)).toMatchObject({id: RUNNER_ID, active_lease_count: 0, assigned_policy: POLICY});
});

test("converts a relative event window to a wire instant", async () => {
  fixture.respond = () => Response.json({items: [], total: null, next_cursor: null});
  const before = Date.now();
  const result = await fixture.run(["runners", "events", RUNNER_ID, "--since", "2h", "--event-type", "runner_online", "--json"]);
  expect(result.code).toBe(0);
  const since = fixture.calls[0]?.url.searchParams.get("since") ?? "";
  expect(since).toMatch(/^\d{4}-\d\d-\d\dT.*Z$/u);
  expect(Date.parse(since)).toBeGreaterThanOrEqual(before - 7_200_000);
  expect(Date.parse(since)).toBeLessThanOrEqual(Date.now() - 7_200_000);
  expect(fixture.calls[0]?.url.searchParams.get("event_type")).toBe("runner_online");
});

test.each(["nonsense", "2026-02-30T00:00:00Z", "2026-11-10T24:00:00Z", "-2h", "2h30m"])("refuses invalid --since %s without a request", async (since) => {
  const result = await fixture.run(["runners", "events", RUNNER_ID, `--since=${since}`, "--json"]);
  expect(result.code).toBe(4); expect(result.stderr).toContain("INVALID_ARGUMENT"); expect(fixture.calls).toEqual([]);
});

test("refuses reversed event bounds before HTTP", async () => {
  const result = await fixture.run(["runners", "events", RUNNER_ID, "--since", "2026-11-10T01:00:00Z", "--until", "2026-11-10T00:00:00Z", "--json"]);
  expect(result.code).toBe(4); expect(result.stderr).toContain("--until must be at or after --since"); expect(fixture.calls).toEqual([]);
});

test("refuses an unknown runner event type before HTTP", async () => {
  const result = await fixture.run(["runners", "events", RUNNER_ID, "--event-type", "invented_event", "--json"]);
  expect(result.code).toBe(4);
  expect(JSON.parse(result.stderr).error.code).toBe("INVALID_ARGUMENT");
  expect(fixture.calls).toEqual([]);
});

test("passes workspace and fleet lease filters under their wire names", async () => {
  fixture.respond = () => Response.json({items: [], total: 0, next_cursor: null});
  const result = await fixture.run(["runners", "leases", RUNNER_ID, "--workspace", WORKSPACE_ID, "--fleet", "Support triage", "--json"]);
  expect(result.code).toBe(0);
  expect(fixture.calls[0]?.url.searchParams.get("workspace_id")).toBe(WORKSPACE_ID);
  expect(fixture.calls[0]?.url.searchParams.get("fleet")).toBe("Support triage");
});

test.each([["cordon", "cordon"], ["drain", "drain"], ["revoke", "revoke"], ["self-test", "self_test"]])("sends the %s state action", async (command, action) => {
  fixture.respond = () => Response.json({id: RUNNER_ID, status: "cordoned"});
  const result = await fixture.run(["runners", command, RUNNER_ID, "--json"]);
  expect(result.code).toBe(0); expect(fixture.calls[0]).toMatchObject({method: "PATCH", body: {action}});
});

test("clears labels explicitly and refuses an empty edit", async () => {
  fixture.respond = () => Response.json({id: RUNNER_ID, status: "active", labels: []});
  const cleared = await fixture.run(["runners", "update", RUNNER_ID, "--labels", "", "--json"]);
  expect(cleared.code).toBe(0); expect(fixture.calls[0]?.body).toEqual({labels: []});
  fixture.calls.length = 0;
  const empty = await fixture.run(["runners", "update", RUNNER_ID, "--json"]);
  expect(empty.code).toBe(4); expect(empty.stderr).toContain("Supply --policy or --labels"); expect(fixture.calls).toEqual([]);
});

test.each(["enroll", "rotate-token"])("saves the %s credential privately without printing it", async (command) => {
  const token = "agt_r_" + "a".repeat(64);
  fixture.respond = () => Response.json(command === "enroll" ? {runner_id: RUNNER_ID, runner_token: token, assigned_policy: POLICY} : {id: RUNNER_ID, runner_token: token});
  const file = join(fixture.state, "runner-token");
  const args = command === "enroll" ? ["--host-id", "build-host", "--policy", "-"] : [RUNNER_ID];
  const result = await fixture.run(["runners", command, ...args, "--token-out", file, "--json"], JSON.stringify(POLICY));
  expect(result.code).toBe(0); expect(readFileSync(file, "utf8")).toBe(`${token}\n`);
  expect(statSync(file).mode & 0o777).toBe(0o600); expect(result.stdout + result.stderr).not.toContain(token);
  expect(JSON.parse(result.stdout).token_file).toBe(file); expect(fixture.calls).toHaveLength(1);
});

test("reserves token storage before mutation and never overwrites a file", async () => {
  const file = join(fixture.state, "runner-token"); writeFileSync(file, "keep");
  const result = await fixture.run(["runners", "rotate-token", RUNNER_ID, "--token-out", file, "--json"]);
  expect(result.code).toBe(4); expect(result.stderr).toContain("existing file is never overwritten");
  expect(readFileSync(file, "utf8")).toBe("keep"); expect(fixture.calls).toEqual([]);
});

test("does not retry a token mutation after an uncertain server response", async () => {
  fixture.respond = () => Response.json({error_code: "UZ-INTERNAL-001", user_message: "Unavailable"}, {status: 503});
  const file = join(fixture.state, "runner-token");
  const result = await fixture.run(["runners", "rotate-token", RUNNER_ID, "--token-out", file, "--json"]);
  expect(result.code).not.toBe(0); expect(fixture.calls).toHaveLength(1); expect(existsSync(file)).toBe(false);
});

test("rejects an unknown runner state instead of presenting a successful read", async () => {
  fixture.respond = () => Response.json({...RUNNER_DETAIL, status: "mystery"});
  const result = await fixture.run(["runners", "show", RUNNER_ID, "--json"]);
  expect(result.code).not.toBe(0); expect(result.stderr).toContain("supported API shape"); expect(result.stdout).toBe("");
});
