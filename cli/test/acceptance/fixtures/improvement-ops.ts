// Improve snapshots an exact setup and applies the candidate explicitly after review.

import { Schema } from "effect";
import { v7 as uuidv7 } from "uuid";
import { AppliedSchema, DraftSchema, FleetSchema } from "../../../src/commands/chat/protocol.ts";
import { nativeRequest, readAuthContext } from "./native-api.ts";
import { authoringModel, commandValues, inspectDraft } from "./native-setup.ts";
import { runFleetctl } from "./cli.js";

type Env = Readonly<Record<string, string>>;
const COMMAND_TIMEOUT_MS = 120_000;

export async function applyReviewedImprovement(env: Env, fleetId: string) {
  const ctx = await readAuthContext(env);
  const current = Schema.decodeUnknownSync(FleetSchema)(await nativeRequest(ctx, `/fleets/${fleetId}`));
  const draft = Schema.decodeUnknownSync(DraftSchema)(await nativeRequest(ctx, "/drafts", "POST", {
    source: {kind: "fleet", fleet_id: fleetId, setup_revision: current.setup_revision},
    authoring_model_ref: await authoringModel(ctx), operation_id: uuidv7(),
  }));
  if (!draft.candidate) throw new Error("Improve returned no native candidate");
  const instructions = draft.candidate.definition.instructions_md;
  await nativeRequest(ctx, `/drafts/${draft.id}`, "PATCH", {instruction_edits: [{start_byte: 0,
    end_byte: Buffer.byteLength(instructions), text: `${instructions}\nReview marker ${uuidv7()}`} ]}, {"If-Match": draft.etag});
  const review = await inspectDraft(env, draft.id);
  const result = await runFleetctl(["--json", "chat", draft.id, "--no-input", "--apply", "--review-digest", review.review_digest],
    {env, timeoutMs: COMMAND_TIMEOUT_MS});
  const envelope = commandValues(result).find(Schema.is(AppliedSchema));
  if (!envelope) throw new Error("Apply returned no setup revision");
  return {...result, envelope, previousRevision: current.setup_revision};
}

export async function resolveFleetName(env: Env, fleetId: string): Promise<string> {
  const ctx = await readAuthContext(env);
  return Schema.decodeUnknownSync(FleetSchema)(await nativeRequest(ctx, `/fleets/${fleetId}`)).name;
}
