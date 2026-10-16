// Explicit terminal publication creates immutable revisions; browsing never applies them.

import { describe, it, beforeAll, afterAll } from "bun:test";
import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Schema } from "effect";
import { v7 as uuidv7 } from "uuid";
import { DraftSchema, PublishedSchema } from "../../src/commands/chat/protocol.ts";
import { composeEnv, runFleetctl } from "./fixtures/cli.js";
import { ACCEPTANCE_RUN_PREFIX } from "./fixtures/constants.ts";
import { attachJwt } from "./fixtures/clerk-admin.ts";
import { resolveAcceptanceEnv, resolveClerkSecret, resolveFixtureEmail } from "./global-setup.ts";
import { hydrateWorkspacesForToken } from "./fixtures/workspace-hydration.ts";
import { readAuthContext, nativeRequest, type AuthContext } from "./fixtures/native-api.ts";
import { fixtureDefinition, createNativeDraft, inspectDraft, commandValues } from "./fixtures/native-setup.ts";
import { cleanWorkspaceLibraryEntries } from "./fixtures/teardown.ts";

const live = (process.env.AGENTSFLEET_ACCEPTANCE_TARGET ?? "").startsWith("https://");
if (!live) describe("Library publication", () => {it.skip("requires the preflighted live lane", () => {});});
else describe("explicit Library revisions", () => {
  let env: Record<string, string> = {}; let ctx: AuthContext; let state = "";
  beforeAll(async () => {
    const apiUrl = resolveAcceptanceEnv().apiUrl;
    const session = await attachJwt(resolveClerkSecret(), {email: resolveFixtureEmail("regular")});
    state = await mkdtemp(join(tmpdir(), "agentsfleet-publication-"));
    env = composeEnv({AGENTSFLEET_API_URL: apiUrl, AGENTSFLEET_STATE_DIR: state, NO_COLOR: "1"});
    await hydrateWorkspacesForToken({apiUrl, token: session.sessionJwt, stateDir: state});
    ctx = await readAuthContext(env);
  });
  afterAll(async () => {
    try {if (ctx) await cleanWorkspaceLibraryEntries(env, {workspaceId: ctx.workspaceId, runPrefix: ACCEPTANCE_RUN_PREFIX});}
    finally {if (state) await rm(state, {recursive: true, force: true});}
  });
  async function publish(draft: string, label: string, previous?: typeof PublishedSchema.Type) {
    const review = await inspectDraft(env, draft);
    const result = await runFleetctl(["--json", "chat", draft, "--no-input", "--publish", label,
      "--review-digest", review.review_digest, ...(previous ? ["--publish-to", previous.library_id, "--library-etag", previous.etag] : [])], {env});
    const publication = commandValues(result).find(Schema.is(PublishedSchema));
    if (!publication) throw new Error("Publication returned no immutable revision identity");
    return publication;
  }
  it("publishes and lists the exact identifier and pinned revision", async () => {
    const name = `${ACCEPTANCE_RUN_PREFIX}-publish-${uuidv7().slice(-8)}`;
    const draft = await createNativeDraft(ctx, fixtureDefinition(name));
    const published = await publish(draft.id, name);
    const listed = await runFleetctl(["library", "--json"], {env});
    assert.equal(listed.code, 0);
    assert.match(listed.stdout, new RegExp(published.library_id));
    assert.match(listed.stdout, new RegExp(published.revision_id));
  }, 120_000);
  it("appends V2 to the same entry and preserves the complete V1 recipe", async () => {
    const name = `${ACCEPTANCE_RUN_PREFIX}-revisions-${uuidv7().slice(-8)}`;
    const original = fixtureDefinition(name);
    const draft = await createNativeDraft(ctx, original);
    const first = await publish(draft.id, name);
    const current = Schema.decodeUnknownSync(DraftSchema)(await nativeRequest(ctx, `/drafts/${draft.id}`));
    await nativeRequest(ctx, `/drafts/${draft.id}`, "PATCH", {instruction_edits: [{start_byte: 0,
      end_byte: Buffer.byteLength(original.instructions_md), text: "Keep the revised instructions."}]}, {"If-Match": current.etag});
    const second = await publish(draft.id, name, first);
    assert.equal(second.library_id, first.library_id); assert.notEqual(second.revision_id, first.revision_id);
    const read = Schema.Struct({head: Schema.Struct({id: Schema.String, definition: Schema.Struct({instructions_md: Schema.String})})});
    const firstRead = Schema.decodeUnknownSync(read)(await nativeRequest(ctx, `/fleet-libraries/${first.library_id}?revision_id=${first.revision_id}`));
    const secondRead = Schema.decodeUnknownSync(read)(await nativeRequest(ctx, `/fleet-libraries/${second.library_id}?revision_id=${second.revision_id}`));
    assert.equal(firstRead.head.id, first.revision_id); assert.equal(firstRead.head.definition.instructions_md, original.instructions_md);
    assert.equal(secondRead.head.definition.instructions_md, "Keep the revised instructions.");
  }, 120_000);
});
