// Seed a typed candidate, then exercise explicit terminal Review and Save.

import type { AuthContext } from "./native-api.ts";
import type { RunResult } from "./cli.js";
import { Schema } from "effect";
import { v7 as uuidv7 } from "uuid";
import { DefinitionSchema, DraftSchema, FleetSchema, ReviewSchema } from "../../../src/commands/chat/protocol.ts";
import { readAuthContext, nativeRequest } from "./native-api.ts";
import { runFleetctl } from "./cli.js";
import { NANOS_PER_USD } from "../../../src/constants/billing.ts";

const modelSchema = Schema.Struct({ref: Schema.String, eligibility: Schema.Struct({authoring: Schema.Boolean, runtime: Schema.Boolean})});
const modelPage = Schema.Struct({items: Schema.Array(modelSchema), next_cursor: Schema.NullOr(Schema.String)});
const PAGE_MAX = 100;
const COMMAND_TIMEOUT_MS = 120_000;

export function fixtureDefinition(name: string, credential?: string): typeof DefinitionSchema.Type {
  return {schema_version: 1, description: "Acceptance probe",
    instructions_md: "Reply with ACK followed by the request text. Do not call tools.",
    config: {name, "x-agentsfleet": {triggers: [], tools: [], credentials: credential ? [credential] : [],
      budget: {daily_nanos: String(NANOS_PER_USD)}}}};
}

export async function authoringModel(ctx: AuthContext): Promise<string> {
  let cursor: string | null = null; const seen = new Set<string>();
  for (let page = 0; page < PAGE_MAX; page++) {
    const query = new URLSearchParams({limit: "100"}); if (cursor) query.set("starting_after", cursor);
    const models = Schema.decodeUnknownSync(modelPage)(await nativeRequest(ctx, `/models?${query}`));
    const model = models.items.find((entry) => entry.eligibility.authoring && entry.eligibility.runtime);
    if (model) return model.ref;
    if (!models.next_cursor || seen.has(models.next_cursor)) break;
    cursor = models.next_cursor; seen.add(cursor);
  }
  throw new Error("No eligible owner model for native acceptance setup");
}

export async function createNativeDraft(ctx: AuthContext, definition: typeof DefinitionSchema.Type, runtime?: string) {
  const model = await authoringModel(ctx);
  const draft = Schema.decodeUnknownSync(DraftSchema)(await nativeRequest(ctx, "/drafts", "POST", {
    source: {kind: "definition", definition}, authoring_model_ref: model, operation_id: uuidv7(),
  }));
  return Schema.decodeUnknownSync(DraftSchema)(await nativeRequest(ctx, `/drafts/${draft.id}`, "PATCH",
    {runtime_model_ref: runtime ?? model}, {"If-Match": draft.etag}));
}

export function commandValues(result: RunResult): unknown[] {
  if (result.code !== 0) throw new Error(`Terminal fixture command refused with exit ${result.code}`);
  return result.stdout.trim().split("\n").filter(Boolean).map((line) => JSON.parse(line));
}

export async function inspectDraft(env: Readonly<Record<string, string>>, draft: string, name?: string) {
  const args = ["--json", "chat", draft, "--no-input", "--review", ...(name ? ["--name", name] : [])];
  const values = commandValues(await runFleetctl(args, {env, timeoutMs: COMMAND_TIMEOUT_MS}));
  const review = values.find(Schema.is(ReviewSchema));
  if (!review) throw new Error("Terminal fixture returned no reviewed candidate");
  return review;
}

export async function saveNativeFleet(env: Readonly<Record<string, string>>, definition: typeof DefinitionSchema.Type, runtime?: string) {
  const ctx = await readAuthContext(env);
  const draft = await createNativeDraft(ctx, definition, runtime);
  const review = await inspectDraft(env, draft.id);
  const result = await runFleetctl(["--json", "chat", draft.id, "--no-input", "--save", "--review-digest", review.review_digest],
    {env, timeoutMs: COMMAND_TIMEOUT_MS});
  // Partial setup is observable to callers that exercise missing-access recovery.
  if (result.code !== 0 && result.code !== 1) throw new Error(`Native Save refused with exit ${result.code}`);
  const fleet = result.stdout.trim().split("\n").filter(Boolean).map((line): unknown => JSON.parse(line)).find(Schema.is(FleetSchema));
  if (!fleet) throw new Error("Native Save returned no fleet");
  return fleet;
}
