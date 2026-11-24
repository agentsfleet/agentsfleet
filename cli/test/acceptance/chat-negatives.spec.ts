// Negative terminal journeys: a draft flag that is gone, a create with no
// message, and a second fleet under a taken name.

import { describe, it, beforeAll, afterAll } from "bun:test";
import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { v7 as uuidv7 } from "uuid";
import { composeEnv, runFleetctl } from "./fixtures/cli.js";
import { ACCEPTANCE_RUN_PREFIX } from "./fixtures/constants.ts";
import { attachJwt } from "./fixtures/clerk-admin.ts";
import { resolveAcceptanceEnv, resolveClerkSecret, resolveFixtureEmail } from "./global-setup.ts";
import { hydrateWorkspacesForToken } from "./fixtures/workspace-hydration.ts";
import { readAuthContext, type AuthContext } from "./fixtures/native-api.ts";
import { createFleetFromCli, PROBE_MESSAGE } from "./fixtures/fleet-create.ts";
import { assertNoSecretLeak } from "./fixtures/negatives.ts";
import { cleanWorkspaceFleets } from "./fixtures/teardown.ts";
import { REJECTION_CODE } from "../../src/constants/rejection.ts";

// af_core/src/error_code/fleet.rs: a name this workspace already holds.
const CODE_NAME_TAKEN = "UZ-AGT-006" as const;

const live = (process.env.AGENTSFLEET_ACCEPTANCE_TARGET ?? "").startsWith("https://");
if (!live) describe("chat negatives", () => {it.skip("requires the preflighted live lane", () => {});});
else describe("chat and fleet create refuse what they cannot do", () => {
  let env: Record<string, string> = {};
  let context: AuthContext;
  let state = "";
  beforeAll(async () => {
    const apiUrl = resolveAcceptanceEnv().apiUrl;
    const session = await attachJwt(resolveClerkSecret(), {email: resolveFixtureEmail("regular")});
    state = await mkdtemp(join(tmpdir(), "agentsfleet-chat-neg-"));
    env = composeEnv({AGENTSFLEET_API_URL: apiUrl, AGENTSFLEET_STATE_DIR: state, NO_COLOR: "1"});
    await hydrateWorkspacesForToken({apiUrl, token: session.sessionJwt, stateDir: state});
    context = await readAuthContext(env);
  });
  afterAll(async () => {
    try { if (context) await cleanWorkspaceFleets(env, {workspaceId: context.workspaceId, runPrefix: ACCEPTANCE_RUN_PREFIX}); }
    finally { if (state) await rm(state, {recursive: true, force: true}); }
  });
  async function run(args: string[]) {
    const result = await runFleetctl(["--json", ...args], {env});
    assertNoSecretLeak(result, context.token); return result;
  }
  it("chat --save is refused, naming fleet create", async () => {
    const result = await run(["chat", "--save"]);
    assert.equal(result.code, 4);
    const error = JSON.parse(result.stderr).error as {code: string; suggestion: string};
    assert.equal(error.code, REJECTION_CODE.invalidArgument);
    assert.match(error.suggestion, /agentsfleet fleet create "<message>"/);
    assert.equal(result.stdout.trim(), "");
  });
  it("fleet create with no message is refused before any fleet exists", async () => {
    const result = await run(["fleet", "create"]);
    assert.equal(result.code, 4);
    assert.match(result.stderr, /message is required/i);
    assert.equal(result.stdout.trim(), "");
  });
  it("refuses a taken name without reporting a second fleet", async () => {
    const name = `${ACCEPTANCE_RUN_PREFIX}-duplicate-${uuidv7().slice(-8)}`;
    const first = await createFleetFromCli(env, {name});
    const second = await run(["fleet", "create", PROBE_MESSAGE, "--name", name]);
    assert.equal(second.code, 3);
    assert.equal(JSON.parse(second.stderr).error.code, CODE_NAME_TAKEN);
    assert.equal(second.stdout.trim(), "");
    const listed = await run(["fleet", "list"]);
    const named = (JSON.parse(listed.stdout) as {items: ReadonlyArray<{id: string; name: string}>}).items
      .filter((fleet) => fleet.name === name);
    assert.deepEqual(named.map((fleet) => fleet.id), [first.fleet_id]);
  }, 120_000);
});
