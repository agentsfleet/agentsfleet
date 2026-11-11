import type { AuthContext } from "./fixtures/native-api.ts";
import { afterAll, beforeAll, describe, it } from "bun:test";
import assert from "node:assert/strict";
import { mkdtemp, rm, stat } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Schema } from "effect";
import { v7 as uuidv7 } from "uuid";
import { CLI_CREDENTIAL_PATTERN } from "../../src/constants/cli-credential.ts";
import { PublishedSchema } from "../../src/commands/chat/protocol.ts";
import { RunnerDetailSchema } from "../../src/commands/platform/runner-schema.ts";
import { composeEnv, runFleetctl } from "./fixtures/cli.js";
import { attachJwt } from "./fixtures/clerk-admin.ts";
import { hydrateWorkspacesForToken } from "./fixtures/workspace-hydration.ts";
import { readAuthContext, nativeRequest } from "./fixtures/native-api.ts";
import { createNativeDraft, fixtureDefinition, inspectDraft } from "./fixtures/native-setup.ts";
import { resolveAcceptanceEnv, resolveClerkSecret, resolveFixtureEmail } from "./global-setup.ts";

const SCOPES = ["runner:enroll", "runner:write", "model:admin", "platform-key:admin", "platform-library:write"] as const;
const POLICY = {sandbox_tier: "landlock_full", network_policy: "deny_all_egress", registry_allowlist: [],
  worker_count: 1, extra_binds: [], durable_tasks_enabled: true};
const libraryEntry = Schema.Struct({card: Schema.Struct({id: Schema.String, etag: Schema.String}), published: Schema.Boolean});
const rotated = Schema.Struct({id: Schema.String, token_file: Schema.String});
const signedIn = Schema.Struct({authenticated: Schema.Boolean, credential: Schema.String,
  source: Schema.String, scopes: Schema.Array(Schema.String)});
const live = (process.env.AGENTSFLEET_ACCEPTANCE_TARGET ?? "").startsWith("https://");

if (!live) describe("Platform operator commands", () => {it.skip("requires the preflighted live lane", () => {});});
else describe("Platform operator commands", () => {
  let env: Record<string, string> = {};
  let ordinaryEnv: Record<string, string> = {};
  let state = "";
  let ordinaryState = "";
  let ctx: AuthContext;
  let runnerId: string | undefined;
  let libraryId: string | undefined;
  let draftId: string | undefined;

  async function command(args: ReadonlyArray<string>): Promise<unknown> {
    const result = await runFleetctl(["--json", "platform", ...args], {env});
    assert.equal(result.code, 0, result.stderr);
    assert.doesNotMatch(result.stdout, /Contacting platform|\r/u);
    return JSON.parse(result.stdout);
  }

  beforeAll(async () => {
    const apiUrl = resolveAcceptanceEnv().apiUrl;
    const email = resolveFixtureEmail("regular");
    // This identity owns its resources; the release-preflight operator stays read-only.
    const operator = await attachJwt(resolveClerkSecret(), {email: email.replace("@", "+cli-platform@"), role: "cli-platform", scopes: SCOPES});
    state = await mkdtemp(join(tmpdir(), "agentsfleet-platform-live-"));
    env = composeEnv({AGENTSFLEET_API_URL: apiUrl, AGENTSFLEET_STATE_DIR: state, NO_COLOR: "1"});
    await hydrateWorkspacesForToken({apiUrl, token: operator.sessionJwt, stateDir: state});
    ctx = await readAuthContext(env);
    assert.ok(CLI_CREDENTIAL_PATTERN.test(ctx.token), "operator fixture must use an issued CLI credential");
    const ordinary = await attachJwt(resolveClerkSecret(), {email});
    ordinaryState = await mkdtemp(join(tmpdir(), "agentsfleet-platform-refusal-"));
    ordinaryEnv = composeEnv({AGENTSFLEET_API_URL: apiUrl, AGENTSFLEET_STATE_DIR: ordinaryState, NO_COLOR: "1"});
    await hydrateWorkspacesForToken({apiUrl, token: ordinary.sessionJwt, stateDir: ordinaryState});
    const ordinaryContext = await readAuthContext(ordinaryEnv);
    assert.ok(CLI_CREDENTIAL_PATTERN.test(ordinaryContext.token), "tenant fixture must use an issued CLI credential");
  });

  afterAll(async () => {
    try {
      if (runnerId) {
        await command(["runners", "revoke", runnerId]);
        await command(["runners", "delete", runnerId]);
      }
      if (libraryId) {
        let entry = Schema.decodeUnknownSync(libraryEntry)(await command(["fleet-library", "show", libraryId]));
        if (entry.published) entry = Schema.decodeUnknownSync(libraryEntry)(await command(["fleet-library", "unpublish", libraryId, "--if-match", entry.card.etag]));
        await command(["fleet-library", "delete", libraryId, "--if-match", entry.card.etag]);
      }
      if (draftId) await nativeRequest(ctx, `/drafts/${draftId}`, "DELETE");
    } finally {
      if (state) await rm(state, {recursive: true, force: true});
      if (ordinaryState) await rm(ordinaryState, {recursive: true, force: true});
    }
  });

  it("authenticates issued operator and tenant CLI credentials with their granted scopes", async () => {
    for (const [credentialEnv, operator] of [[env, true], [ordinaryEnv, false]] as const) {
      const result = await runFleetctl(["--json", "whoami"], {env: credentialEnv});
      assert.equal(result.code, 0, result.stderr);
      const identity = Schema.decodeUnknownSync(signedIn)(JSON.parse(result.stdout));
      assert.equal(identity.authenticated, true);
      assert.equal(identity.credential, "cli_credential");
      assert.equal(identity.source, "file");
      for (const scope of [...SCOPES, "runner:read"]) assert.equal(identity.scopes.includes(scope), operator, scope);
    }
  });

  it.each(["runners", "models", "keys", "fleet-library"])("refuses %s administration for a tenant identity", async group => {
    const result = await runFleetctl(["--json", "platform", group, "list"], {env: ordinaryEnv});
    assert.equal(result.code, 3);
    const error = Schema.decodeUnknownSync(Schema.Struct({error: Schema.Struct({code: Schema.String})}))(JSON.parse(result.stderr));
    assert.equal(error.error.code, "UZ-AUTH-022");
    assert.equal(result.stdout, "");
  });

  it("enrolls an isolated host, changes its policy, rotates its token, and retires it", async () => {
    const file = join(state, "policy.json");
    const tokenFile = join(state, "runner-token");
    await Bun.write(file, JSON.stringify(POLICY));
    const enrollment = Schema.decodeUnknownSync(Schema.Struct({runner_id: Schema.String, token_file: Schema.String}))(
      await command(["runners", "enroll", "--host-id", `cli-acceptance-${uuidv7()}`, "--policy", file, "--token-out", tokenFile]));
    runnerId = enrollment.runner_id;
    assert.equal((await stat(tokenFile)).mode & 0o777, 0o600);
    const token = (await Bun.file(tokenFile).text()).trim();
    assert.match(token, /^agt_r/u);
    const detail = Schema.decodeUnknownSync(RunnerDetailSchema)(await command(["runners", "show", runnerId]));
    assert.equal(detail.status, "active"); assert.equal(detail.assigned_policy.worker_count, 1);
    await Bun.write(file, JSON.stringify({...POLICY, worker_count: 2}));
    await command(["runners", "update", runnerId, "--policy", file, "--labels", "cli-acceptance"]);
    const updated = Schema.decodeUnknownSync(RunnerDetailSchema)(await command(["runners", "show", runnerId]));
    assert.equal(updated.assigned_policy.worker_count, 2); assert.deepEqual(updated.labels, ["cli-acceptance"]);
    const nextFile = join(state, "rotated-token");
    const rotation = Schema.decodeUnknownSync(rotated)(await command(["runners", "rotate-token", runnerId, "--token-out", nextFile]));
    assert.equal(rotation.token_file, nextFile);
    assert.equal((await stat(nextFile)).mode & 0o777, 0o600);
    assert.notEqual((await Bun.file(nextFile).text()).trim(), token);
    await command(["runners", "events", runnerId, "--since", "1h"]);
    await command(["runners", "leases", runnerId]);
    for (const action of ["cordon", "drain", "revoke"]) await command(["runners", action, runnerId]);
    const revoked = Schema.decodeUnknownSync(RunnerDetailSchema)(await command(["runners", "show", runnerId]));
    assert.equal(revoked.status, "revoked");
    await command(["runners", "delete", runnerId]); runnerId = undefined;
  });

  it("reads the live shared model catalogue and provider defaults", async () => {
    const page = Schema.decodeUnknownSync(Schema.Struct({models: Schema.Array(Schema.Struct({id: Schema.String}))}))(
      await command(["models", "list", "--limit", "1"]));
    if (page.models[0]) {
      const model = Schema.decodeUnknownSync(Schema.Struct({id: Schema.String}))(await command(["models", "show", page.models[0].id]));
      assert.equal(model.id, page.models[0].id);
    }
    const keys = Schema.decodeUnknownSync(Schema.Struct({keys: Schema.Array(Schema.Unknown)}))(await command(["keys", "list"]));
    assert.ok(Array.isArray(keys.keys));
  });

  it("stages a reviewed candidate, rejects a stale tag, publishes, withdraws, and deletes it", async () => {
    const name = `cli-acceptance-${uuidv7().slice(-8)}`;
    const draft = await createNativeDraft(ctx, fixtureDefinition(name)); draftId = draft.id;
    const reviewed = Schema.decodeUnknownSync(Schema.Struct({draft_candidate_id: Schema.String}))(await inspectDraft(env, draft.id));
    const publication = Schema.decodeUnknownSync(PublishedSchema)(await command(["fleet-library", "stage", "--workspace", ctx.workspaceId,
      "--candidate-id", reviewed.draft_candidate_id, "--label", name, "--operation-id", uuidv7()]));
    libraryId = publication.library_id;
    let entry = Schema.decodeUnknownSync(libraryEntry)(await command(["fleet-library", "show", libraryId]));
    assert.equal(entry.published, false);
    entry = Schema.decodeUnknownSync(libraryEntry)(await command(["fleet-library", "publish", libraryId, "--if-match", entry.card.etag]));
    assert.equal(entry.published, true);
    const stale = await runFleetctl(["--json", "platform", "fleet-library", "unpublish", libraryId, "--if-match", publication.etag], {env});
    assert.equal(stale.code, 3);
    const failure = Schema.decodeUnknownSync(Schema.Struct({error: Schema.Struct({code: Schema.String})}))(JSON.parse(stale.stderr));
    assert.equal(failure.error.code, "UZ-AUTHOR-003");
    entry = Schema.decodeUnknownSync(libraryEntry)(await command(["fleet-library", "unpublish", libraryId, "--if-match", entry.card.etag]));
    assert.equal(entry.published, false);
    await command(["fleet-library", "delete", libraryId, "--if-match", entry.card.etag]); libraryId = undefined;
  }, 120_000);
});
