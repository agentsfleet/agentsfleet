import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Readable } from "node:stream";
import { runCli } from "../src/cli.ts";

export const RUNNER_ID = "0199a801-1000-7000-8000-000000000001";
export const WORKSPACE_ID = "0199a801-1000-7000-8000-000000000002";
export const INSTANT = "2026-11-10T00:00:00Z";
export const POLICY = {
  sandbox_tier: "landlock_full", network_policy: "deny_all_egress", registry_allowlist: [],
  worker_count: 1, extra_binds: [], durable_tasks_enabled: true,
};
export const RUNNER = {
  id: RUNNER_ID, host_id: "build-host", sandbox_tier: "landlock_full", status: "active", liveness: "online",
  labels: [], last_seen_at: INSTANT, created_at: INSTANT, assigned_policy: POLICY,
  achievable: null, degraded: false, degraded_reason: null,
};
export const RUNNER_DETAIL = {
  ...RUNNER, active_lease_count: 0, active_fleet_count: 0, leases_acquired: 0, leases_succeeded: 0,
  leases_failed: 0, leases_expired: 0, selftest_requested_at: null, selftest_completed_at: null, selftest: null,
};
export const MODEL = {
  id: RUNNER_ID, provider: "example", model_id: "reasoning-model", metadata: {
    display_name: "Reasoning model", context_cap_tokens: 128000, context_source_url: "https://example.com/models",
    context_verified_at: INSTANT, advanced: false,
  }, prices: [{effective_from: INSTANT, effective_until: null, prompt_tokens_above: 0,
    input_nanos_per_mtok: "1000000000", cached_input_nanos_per_mtok: "0", output_nanos_per_mtok: "2000000000",
    source_url: "https://example.com/prices", verified_at: INSTANT, conditions: ""}],
};
export const LIBRARY_ENTRY = {card: {
  id: RUNNER_ID, name: "Support triage", description: "Sort incoming requests", visibility: "platform_draft",
  head_revision_id: WORKSPACE_ID, etag: '"inspected"', requirements: {}, created_at: INSTANT, required_credentials_reasons: {},
}, published: false, updated_at: INSTANT};
export type PlatformCall = {url: URL; method: string; body: unknown; headers: Headers};

/** Capture only the HTTP boundary; argument parsing and runtime services remain real. */
export class PlatformFixture {
  readonly calls: PlatformCall[] = [];
  readonly state = mkdtempSync(join(tmpdir(), "agentsfleet-platform-"));
  respond: (call: PlatformCall) => Response = () => Response.json({});

  async run(args: readonly string[], input = "", tty = false, inputTTY = false) {
    const stdout: string[] = []; const stderr: string[] = [];
    const fetchImpl = async (resource: string | URL | Request, init?: RequestInit) => {
      const url = new URL(typeof resource === "string" ? resource : resource instanceof URL ? resource.href : resource.url);
      const call = {url, method: init?.method ?? "GET", body: typeof init?.body === "string" ? JSON.parse(init.body) : undefined, headers: new Headers(init?.headers)};
      this.calls.push(call);
      return this.respond(call);
    };
    fetchImpl.preconnect = () => undefined;
    const stdin = Readable.from([input]);
    Object.defineProperty(stdin, "isTTY", {value: inputTTY});
    const code = await runCli(["platform", ...args], {
      env: {AGENTSFLEET_API_URL: "http://127.0.0.1:1", AGENTSFLEET_API_KEY: "afk_unit_test",
        AGENTSFLEET_STATE_DIR: this.state, NO_COLOR: "1"},
      stdout: {write: (value: string) => {stdout.push(value); return true;}, isTTY: tty},
      stderr: {write: (value: string) => {stderr.push(value); return true;}, isTTY: tty},
      stdin,
      fetchImpl,
    });
    return {code, stdout: stdout.join(""), stderr: stderr.join("")};
  }

  dispose() {rmSync(this.state, {recursive: true, force: true});}
}
