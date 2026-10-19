// The daemon validates authored setup; this client validates what it consumes.

import { Effect, Schema } from "effect";
import { instant } from "../../lib/instant.ts";
import { ServerError } from "../../errors/index.ts";

export const DRAFT_STATE = { editing: "editing", saving: "saving", closed: "closed" } as const;
export const MESSAGE_ROLE = { user: "user", assistant: "assistant" } as const;
const FAILED_STATE = "failed";
export const MESSAGE_STATE = {
  queued: "queued", running: "running", completed: "completed", cancelled: "cancelled",
  failed: FAILED_STATE, timedOut: "timed_out",
} as const;
/** `SyncStatus` in rustd/crates/afd_cron/src/model.rs: anything short of `synced` is retried by the schedule's `:sync` action. */
export const SCHEDULE_SYNC = { syncing: "syncing", synced: "synced", failed: FAILED_STATE } as const;
const DECIMAL = /^(0|[1-9][0-9]*)$/;
/** A daemon counter or revision: a decimal string, because nanos outgrow a JavaScript number. */
export const decimal = Schema.String.check(Schema.isPattern(DECIMAL));
const object = Schema.Record(Schema.String, Schema.Unknown);

export const DefinitionSchema = Schema.Struct({
  schema_version: Schema.Literal(1),
  description: Schema.String,
  instructions_md: Schema.String,
  config: object,
});
export const CandidateSchema = Schema.Struct({
  id: Schema.String, revision: decimal, definition: DefinitionSchema,
  canonical_digest: Schema.String, policy_semantics: Schema.String,
});
const action = Schema.Struct({ kind: Schema.String, href: Schema.optionalKey(Schema.String) });
const check = Schema.Struct({
  kind: Schema.String, subject: Schema.String, state: Schema.String,
  next_action: Schema.optionalKey(action),
});
export const ReviewSchema = Schema.Struct({
  effective: Schema.Struct({
    definition: DefinitionSchema, instance_name: Schema.String,
    runtime_model_ref: Schema.NullOr(Schema.String),
    source_revision: Schema.optionalKey(Schema.String),
    target_setup_revision: Schema.optionalKey(decimal),
  }),
  requirements: object, readiness: Schema.Array(check), review_digest: Schema.String,
  policy_semantics: Schema.String, observed_at: instant,
});
export const AcceptedSchema = Schema.Struct({
  status: Schema.Literal("accepted"), sequence: decimal, replayed: Schema.Boolean,
});
export const DraftSchema = Schema.Struct({
  id: Schema.String, state: Schema.Literals(Object.values(DRAFT_STATE)), title: Schema.String,
  etag: Schema.String, authoring_model_ref: Schema.String, cancelled_through: decimal,
  candidate: Schema.optionalKey(CandidateSchema), review: Schema.optionalKey(ReviewSchema),
  linked_fleet_id: Schema.optionalKey(Schema.String), accepted: Schema.optionalKey(AcceptedSchema),
});
export const CreatedSchema = Schema.Struct({...DraftSchema.fields, accepted: AcceptedSchema});
export const MessageSchema = Schema.Struct({
  sequence: decimal, operation_id: Schema.String, role: Schema.Literals(Object.values(MESSAGE_ROLE)),
  state: Schema.Literals(Object.values(MESSAGE_STATE)), text: Schema.optionalKey(Schema.String),
  created_at: instant, failure: Schema.optionalKey(Schema.Struct({
    reason: Schema.String, error_code: Schema.String, recovery_action: Schema.String,
  })),
});
export const MessagesSchema = Schema.Struct({
  items: Schema.Array(MessageSchema), next_cursor: Schema.NullOr(Schema.String),
});
// The published `View` schedule (public/openapi.json). Sync stays an open
// string so a state this build does not know cannot refuse the whole fleet.
export const ScheduleSchema = Schema.Struct({
  schedule_id: Schema.String, fleet_id: Schema.String, sync: Schema.String,
  last_error: Schema.optionalKey(Schema.NullOr(Schema.String)),
});
export const FleetSchema = Schema.Struct({
  id: Schema.String, name: Schema.String, status: Schema.String, etag: Schema.String,
  setup_etag: Schema.String, setup_revision: decimal,
  setup: Schema.Struct({state: Schema.String, checks: Schema.Array(check)}),
  start: object, schedules: Schema.Array(ScheduleSchema),
});
export const PublishedSchema = Schema.Struct({
  library_id: Schema.String, revision_id: Schema.String, head_revision_id: Schema.String, etag: Schema.String,
});
export const AppliedSchema = Schema.Struct({
  fleet_id: Schema.String, setup_revision: decimal, setup_etag: Schema.String, etag: Schema.String,
  schedules: Schema.Array(ScheduleSchema),
});
export type Draft = typeof DraftSchema.Type;
export type Review = typeof ReviewSchema.Type;
export type Message = typeof MessageSchema.Type;
export type Fleet = typeof FleetSchema.Type;
export type Applied = typeof AppliedSchema.Type;
export type Schedule = typeof ScheduleSchema.Type;
export type MessagePage = typeof MessagesSchema.Type;

export function decode<T>(schema: Schema.Codec<T, unknown>, value: unknown): Effect.Effect<T, ServerError> {
  return Schema.decodeUnknownEffect(schema)(value).pipe(Effect.mapError(() => new ServerError({
    detail: "The chat response did not match the supported API shape",
    suggestion: "update the terminal client and verify that it targets the matching daemon build",
    code: "CHAT_RESPONSE_INVALID", status: 502, requestId: null,
  })));
}
