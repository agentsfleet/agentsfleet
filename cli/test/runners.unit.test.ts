import { afterEach, beforeEach, expect, test } from "bun:test";
import { existsSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { PlatformFixture, POLICY, RUNNER_ID } from "./helpers-platform.ts";

const UUIDV7 = /^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/u;
const TOKEN = "agt_r_" + "b".repeat(64);

let fixture: PlatformFixture;
beforeEach(() => {fixture = new PlatformFixture([]);});
afterEach(() => fixture.dispose());

const register = (file: string) => fixture.run(
  ["runners", "register", "--host-id", "build-box-1", "--policy", "-", "--token-out", file, "--json"],
  JSON.stringify(POLICY),
);
const created = () => Response.json({runner_id: RUNNER_ID, runner_token: TOKEN, assigned_policy: POLICY}, {status: 201});

test("runners register saves the token and sends a key", async () => {
  fixture.respond = created;
  const file = join(fixture.state, "runner-token");
  const result = await register(file);
  expect(result.code).toBe(0);
  expect(readFileSync(file, "utf8")).toBe(`${TOKEN}\n`);
  expect(statSync(file).mode & 0o777).toBe(0o600);
  expect(result.stdout + result.stderr).not.toContain(TOKEN);
  expect(fixture.calls).toHaveLength(1);
  expect(fixture.calls[0]?.url.pathname).toBe("/v1/tenants/me/runners");
  expect(fixture.calls[0]?.headers.get("idempotency-key") ?? "").toMatch(UUIDV7);
});

test("runners register replay suggests rotate-token", async () => {
  fixture.respond = () => Response.json({runner_id: RUNNER_ID, assigned_policy: POLICY});
  const file = join(fixture.state, "runner-token");
  const result = await register(file);
  expect(result.code).toBe(3);
  expect(existsSync(file)).toBe(false);
  expect(JSON.parse(result.stderr).error.suggestion).toContain(`agentsfleet runners rotate-token ${RUNNER_ID}`);
});

test("a dropped reply retries with the same key and saves the token once", async () => {
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
  const keys = fixture.calls.map((call) => call.headers.get("idempotency-key"));
  expect(keys[0]).toMatch(UUIDV7);
  expect(keys[1]).toBe(keys[0]);
  expect(readFileSync(file, "utf8")).toBe(`${TOKEN}\n`);
});

test("the tenant group never calls the admin path", async () => {
  fixture.respond = () => Response.json({items: [], total: 0, next_cursor: null});
  const result = await fixture.run(["runners", "list", "--json"]);
  expect(result.code).toBe(0);
  expect(fixture.calls[0]?.url.pathname).toBe("/v1/tenants/me/runners");
});
