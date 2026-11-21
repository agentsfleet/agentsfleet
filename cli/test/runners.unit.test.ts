import { afterEach, beforeEach, expect, test } from "bun:test";
import { existsSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { PlatformFixture, POLICY, RUNNER, RUNNER_ID } from "./helpers-platform.ts";

const TOKEN = "agt_r_" + "b".repeat(64);

let fixture: PlatformFixture;
beforeEach(() => {fixture = new PlatformFixture([]);});
afterEach(() => fixture.dispose());

const register = (file: string) => fixture.run(
  ["runners", "register", "--host-id", "build-box-1", "--policy", "-", "--token-out", file, "--json"],
  JSON.stringify(POLICY),
);
const created = () => Response.json({runner_id: RUNNER_ID, runner_token: TOKEN, assigned_policy: POLICY}, {status: 201});

test("runners register saves the token and sends no key", async () => {
  fixture.respond = created;
  const file = join(fixture.state, "runner-token");
  const result = await register(file);
  expect(result.code).toBe(0);
  expect(readFileSync(file, "utf8")).toBe(`${TOKEN}\n`);
  expect(statSync(file).mode & 0o777).toBe(0o600);
  expect(result.stdout + result.stderr).not.toContain(TOKEN);
  expect(fixture.calls).toHaveLength(1);
  expect(fixture.calls[0]?.url.pathname).toBe("/v1/tenants/me/runners");
  expect(fixture.calls[0]?.headers.get("idempotency-key")).toBeNull();
});

test("a host registered before answers 409 and suggests rotate-token", async () => {
  fixture.respond = () => Response.json(
    {error_code: "UZ-RUN-033", detail: "this host is already registered", current_state: "registered"},
    {status: 409, headers: {location: `/v1/tenants/me/runners/${RUNNER_ID}`}},
  );
  const file = join(fixture.state, "runner-token");
  const result = await register(file);
  expect(result.code).toBe(3);
  expect(existsSync(file)).toBe(false);
  const error = JSON.parse(result.stderr).error;
  expect(error.code).toBe("UZ-RUN-033");
  expect(error.suggestion).toContain("agentsfleet runners rotate-token");
  expect(fixture.calls).toHaveLength(1);
});

test("a dropped reply retries and saves the token once", async () => {
  let attempt = 0;
  fixture.respond = () => {
    attempt += 1;
    // The shape fetch gives a dropped socket under Bun: a TypeError carrying the code.
    if (attempt === 1) throw Object.assign(new TypeError("socket hang up"), {code: "ECONNRESET"});
    return created();
  };
  const file = join(fixture.state, "runner-token");
  const result = await register(file);
  expect(result.code).toBe(0);
  expect(fixture.calls).toHaveLength(2);
  expect(readFileSync(file, "utf8")).toBe(`${TOKEN}\n`);
});

test("the tenant group never calls the admin path", async () => {
  fixture.respond = () => Response.json({items: [], total: 0, next_cursor: null});
  const result = await fixture.run(["runners", "list", "--json"]);
  expect(result.code).toBe(0);
  expect(fixture.calls[0]?.url.pathname).toBe("/v1/tenants/me/runners");
});

test("runners register without runner:tenant names the scope", async () => {
  fixture.respond = () => Response.json({error_code: "UZ-AUTH-022", detail: "Requires scope runner:tenant"}, {status: 403});
  const result = await register(join(fixture.state, "runner-token"));
  expect(result.code).toBe(3);
  const error = JSON.parse(result.stderr).error;
  expect(JSON.stringify(error)).toContain("runner:tenant");
  expect(error.suggestion).toBeTruthy();
});

test("a tenant's own runner table names no owner: every row is the caller's", async () => {
  fixture.respond = () => Response.json({items: [{...RUNNER, tenant_id: "0199a801-1000-7000-8000-0000000000aa"}], total: 1, next_cursor: null});
  const table = await fixture.run(["runners", "list"], "", true);
  expect(table.code).toBe(0); expect(table.stdout).toContain("build-host"); expect(table.stdout).not.toContain("OWNER");
});
