// The built `agentsfleet runners register` against the mock API: a tenant
// registers its own host, the token lands in a private file and never on a
// stream, and the request carries no Idempotency-Key: a host registers once.
import { expect, test } from "bun:test";
import { mkdtempSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { authedScope, jsonResponse, withMockApi } from "./helpers-fleet-install.ts";
import { composeEnv, runFleetctl } from "./acceptance/fixtures/cli.js";

const RUNNER_ID = "019f0000-0000-7000-8000-000000000246";
const TOKEN = `agt_r${"c".repeat(64)}`;
const POLICY = {
  sandbox_tier: "landlock_full", network_policy: "deny_all_egress", registry_allowlist: [],
  worker_count: 1, extra_binds: [], durable_tasks_enabled: false,
};

test("runners register from the built binary", async () => {
  const work = mkdtempSync(join(tmpdir(), "runners-register-"));
  try {
    const policyFile = join(work, "policy.json");
    const tokenFile = join(work, "runner.token");
    writeFileSync(policyFile, JSON.stringify(POLICY));
    await authedScope((stateDir) => withMockApi({
      "GET /v1/users/me": () => jsonResponse(200, { scopes: ["runner:tenant"] }),
      "POST /v1/tenants/me/runners": () =>
        jsonResponse(201, { runner_id: RUNNER_ID, runner_token: TOKEN, assigned_policy: POLICY }),
    }, async (apiUrl, calls) => {
      const result = await runFleetctl(
        ["runners", "register", "--host-id", "build-box-1", "--policy", policyFile, "--token-out", tokenFile, "--json"],
        { env: composeEnv({ AGENTSFLEET_API_URL: apiUrl, AGENTSFLEET_STATE_DIR: stateDir, NO_COLOR: "1" }) },
      );
      expect(result.code).toBe(0);
      expect(readFileSync(tokenFile, "utf8")).toBe(`${TOKEN}\n`);
      expect(statSync(tokenFile).mode & 0o777).toBe(0o600);
      expect(result.stdout + result.stderr).not.toContain(TOKEN);
      expect(JSON.parse(result.stdout)).toMatchObject({ runner_id: RUNNER_ID, token_file: tokenFile });
      const registrations = calls.filter((call) => call.method === "POST" && call.path === "/v1/tenants/me/runners");
      expect(registrations).toHaveLength(1);
      expect(registrations[0]?.headers["idempotency-key"]).toBeUndefined();
    }));
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
});
