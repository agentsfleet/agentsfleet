import { afterEach, beforeEach, expect, spyOn, test } from "bun:test";
import * as fs from "node:fs";
import { join } from "node:path";
import { PlatformFixture, INSTANT, LIBRARY_ENTRY, MODEL, POLICY, RUNNER, RUNNER_ID } from "./helpers-platform.ts";

let fixture: PlatformFixture;
beforeEach(() => {fixture = new PlatformFixture();});
afterEach(() => fixture.dispose());

test.each(["http://example.test", "https://"])("refuses a malformed platform provider URL %s before a request", async baseUrl => {
  const result = await fixture.run(["keys", "set", "--provider", "openai-compatible", "--workspace", RUNNER_ID,
    "--model", "remote-model", "--base-url", baseUrl]);
  expect(result.code).toBe(4);
  expect(result.stderr).toContain("must be an https URL");
  expect(fixture.calls).toHaveLength(0);
});

test("uses the common HTTPS provider URL flag and URL metavar", async () => {
  fixture.respond = () => Response.json({provider: "openai-compatible", model: "remote-model", active: true});
  const result = await fixture.run(["keys", "set", "--provider", "openai-compatible", "--workspace", RUNNER_ID,
    "--model", "remote-model", "--base-url", "https://example.test/v1"]);
  expect(result.code).toBe(0);
  expect(fixture.calls).toHaveLength(1);
  expect(fixture.calls[0]?.body).toMatchObject({base_url: "https://example.test/v1"});
  const help = await fixture.run(["keys", "set", "--help"]);
  expect(help.stdout).toContain("--base-url <url>");
});

test.each(["runners", "models"])("renders a %s table and its next cursor in a terminal", async group => {
  fixture.respond = () => Response.json(group === "runners" ? {items: [RUNNER], total: 1, next_cursor: RUNNER_ID} : {models: [MODEL], next_cursor: RUNNER_ID});
  const result = await fixture.run([group, "list"], "", true);
  expect(result.code).toBe(0); expect(result.stdout).toContain(group === "runners" ? "build-host" : "Reasoning model");
  expect(result.stdout).toContain(`Next page: --starting-after ${RUNNER_ID}`);
});

test("renders provider defaults and Library entries as terminal tables", async () => {
  fixture.respond = call => Response.json(call.url.pathname.includes("platform-keys")
    ? {keys: [{provider: "example", source_workspace_id: RUNNER_ID, model: null, active: false, updated_at: INSTANT}]} : {entries: [LIBRARY_ENTRY]});
  const keys = await fixture.run(["keys", "list"], "", true);
  expect(keys.code).toBe(0); expect(keys.stdout).toContain("example"); expect(keys.stdout).toContain("No");
  const library = await fixture.run(["fleet-library", "list"], "", true);
  expect(library.code).toBe(0); expect(library.stdout).toContain("Support triage"); expect(library.stdout).toContain('"inspected"');
});

test("renders event history and a continuation cursor", async () => {
  fixture.respond = () => Response.json({items: [{id: RUNNER_ID, runner_id: RUNNER_ID,
    event_type: "runner_online", occurred_at: INSTANT, metadata: {}}], total: 1, next_cursor: RUNNER_ID});
  const result = await fixture.run(["runners", "events", RUNNER_ID], "", true);
  expect(result.code).toBe(0); expect(result.stdout).toContain("runner_online"); expect(result.stdout).toContain("Next page:");
});

test("renders a retained lease with its fleet identifier when the name is gone", async () => {
  fixture.respond = () => Response.json({items: [{id: RUNNER_ID, fleet_id: RUNNER_ID, fleet_name: null, workspace_id: RUNNER_ID,
    destination: "fleet_deleted", event_id: RUNNER_ID, event_type: null, actor: null, outcome: "succeeded", kind: "fresh",
    failure_label: null, failure_detail: null, fencing_token: 1, provider: "example", model: "reasoning-model", posture: "metered",
    metered_input_tokens: 0, metered_cached_tokens: 0, metered_output_tokens: 0, wall_ms: null,
    lease_expires_at: INSTANT, created_at: INSTANT}], total: 1, next_cursor: RUNNER_ID});
  const result = await fixture.run(["runners", "leases", RUNNER_ID], "", true);
  expect(result.code).toBe(0); expect(result.stdout).toContain("succeeded"); expect(result.stdout).toContain(RUNNER_ID); expect(result.stdout).toContain("Next page:");
});

test.each(["runners", "models"])("deletes the inspected %s identifier", async group => {
  fixture.respond = () => new Response(null, {status: 204});
  const result = await fixture.run([group, "delete", RUNNER_ID, "--json"]);
  expect(result.code).toBe(0); expect(JSON.parse(result.stdout)).toEqual({id: RUNNER_ID, deleted: true});
  expect(fixture.calls[0]?.method).toBe("DELETE");
});

test("replaces the complete policy from a file and removes duplicate labels", async () => {
  const file = join(fixture.state, "policy.json"); fs.writeFileSync(file, JSON.stringify(POLICY));
  fixture.respond = () => Response.json({id: RUNNER_ID, status: "active", assigned_policy: POLICY, labels: ["build", "linux"]});
  const result = await fixture.run(["runners", "update", RUNNER_ID, "--policy", file, "--labels", "build, linux,build", "--json"]);
  expect(result.code).toBe(0); expect(fixture.calls[0]?.body).toEqual({assigned_policy: POLICY, labels: ["build", "linux"]});
});

test.each(["missing", "oversized", "terminal"])("refuses %s policy input before HTTP", async kind => {
  const file = join(fixture.state, "policy.json");
  if (kind === "oversized") fs.writeFileSync(file, " ".repeat(1024 * 1024 + 1));
  const result = await fixture.run(["runners", "update", RUNNER_ID, "--policy", kind === "terminal" ? "-" : file, "--json"], "", false, kind === "terminal");
  expect(result.code).toBe(4); expect(JSON.parse(result.stderr).error.code).toBe("INVALID_ARGUMENT"); expect(fixture.calls).toEqual([]);
});

test("refuses a provider key requested from an interactive input", async () => {
  const result = await fixture.run(["keys", "set", "--provider", "example", "--workspace", RUNNER_ID,
    "--model", "reasoning-model", "--api-key-stdin", "--json"], "", false, true);
  expect(result.code).toBe(4); expect(JSON.parse(result.stderr).error.code).toBe("INVALID_ARGUMENT"); expect(fixture.calls).toEqual([]);
});

test("stops oversized piped JSON before a platform request", async () => {
  const result = await fixture.run(["runners", "update", RUNNER_ID, "--policy", "-", "--json"], " ".repeat(1024 * 1024 + 1));
  expect(result.code).toBe(4);
  expect(JSON.parse(result.stderr).error).toMatchObject({code: "INVALID_ARGUMENT", message: "Standard input exceeds 1048576 bytes"});
  expect(fixture.calls).toEqual([]);
});

test("releases a reserved token file when private permissions cannot be applied", async () => {
  const chmod = spyOn(fs, "fchmodSync").mockImplementation(() => {throw new Error("permission denied");});
  const file = join(fixture.state, "token");
  try {
    const result = await fixture.run(["runners", "rotate-token", RUNNER_ID, "--token-out", file, "--json"]);
    expect(chmod).toHaveBeenCalledTimes(1); expect(result.code).toBe(4);
    expect(fs.existsSync(file)).toBe(false); expect(fixture.calls).toEqual([]);
  } finally {chmod.mockRestore();}
});

test("reports an issued token that could not be saved without printing it or retrying", async () => {
  const token = "agt_r_" + "b".repeat(64);
  fixture.respond = () => Response.json({id: RUNNER_ID, runner_token: token});
  const sync = spyOn(fs, "fsyncSync").mockImplementation(() => {throw new Error("disk I/O failed");});
  const file = join(fixture.state, "token");
  try {
    const result = await fixture.run(["runners", "rotate-token", RUNNER_ID, "--token-out", file, "--json"]);
    expect(sync).toHaveBeenCalledTimes(1); expect(result.code).toBe(1);
    expect(JSON.parse(result.stderr).error.suggestion).toContain("do not repeat enrollment");
    expect(result.stdout + result.stderr).not.toContain(token); expect(fixture.calls).toHaveLength(1);
    expect(fs.existsSync(file)).toBe(false);
  } finally {sync.mockRestore();}
});
