// Negative terminal journeys refuse missing input, inaccessible sources and duplicate names.

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
import { fixtureDefinition, createNativeDraft, inspectDraft, saveNativeFleet, authoringModel } from "./fixtures/native-setup.ts";
import { assertNoSecretLeak } from "./fixtures/negatives.ts";
import { cleanWorkspaceFleets } from "./fixtures/teardown.ts";

const live = (process.env.AGENTSFLEET_ACCEPTANCE_TARGET ?? "").startsWith("https://");
if (!live) describe("chat negatives", () => {it.skip("requires the preflighted live lane", () => {});});
else describe("chat refuses unsafe creation", () => {
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
    const result = await runFleetctl(["--json", "chat", ...args], {env});
    assertNoSecretLeak(result, context.token); return result;
  }
  it("requires a message for a new noninteractive conversation", async () => {
    const result = await run(["--no-input"]);
    assert.equal(result.code, 4);
    assert.match(result.stderr, /message is required/i);
  });
  it("cannot review another or nonexistent private draft", async () => {
    const result = await run([uuidv7(), "--no-input", "--review"]);
    assert.notEqual(result.code, 0); assert.equal(result.stdout.trim(), "");
  });
  it("cannot author from an unavailable pinned Library revision", async () => {
    const result = await run(["--library", uuidv7(), "--revision", uuidv7(), "--model", await authoringModel(context),
      "--no-input", "--message", "Inspect this unavailable recipe"]);
    assert.notEqual(result.code, 0); assert.equal(result.stdout.trim(), "");
  });
  it("refuses a taken explicit name without reporting a second fleet", async () => {
    const name = `${ACCEPTANCE_RUN_PREFIX}-duplicate-${uuidv7().slice(-8)}`;
    const first = await saveNativeFleet(env, fixtureDefinition(name));
    assert.equal(first.name, name);
    const second = await createNativeDraft(context, fixtureDefinition(name));
    const review = await inspectDraft(env, second.id);
    const result = await run([second.id, "--no-input", "--save", "--review-digest", review.review_digest]);
    assert.equal(result.code, 3);
    assert.match(result.stderr, /UZ-AGT-006/);
    assert.equal(result.stdout.includes('"setup_etag"'), false);
  }, 120_000);
});
