// Native task responses are validated before text or JSON success is emitted.

import { Effect, Schema } from "effect";
import { ServerError } from "../errors/index.ts";
import { instant } from "../lib/instant.ts";
import { isValidId } from "../lib/id.ts";
import { nanosSchema } from "../lib/nanos.ts";

const identity = Schema.String.check(Schema.makeFilter(isValidId));
const count = Schema.Number.check(Schema.makeFilter((value: number) => Number.isSafeInteger(value) && value >= 0));
const state = Schema.Literals(["ready", "running", "waiting", "blocked", "paused", "done", "cancelled", "exhausted"]);
const question = Schema.Struct({name: Schema.String, question: Schema.Struct({
  type: Schema.Literal("noul"), instructions: Schema.String,
  criteria: Schema.Struct({true: Schema.String, false: Schema.String}),
})});

export const TaskSummarySchema = Schema.Struct({
  id: identity, title: Schema.String, status: state, resource_version: count, generation: count,
  accepted_revision_id: identity, created_at: instant, expires_at: instant,
});
export const TaskPageSchema = Schema.Struct({items: Schema.Array(TaskSummarySchema), next_cursor: Schema.NullOr(Schema.String)});
export const TaskDetailSchema = Schema.Struct({
  title: Schema.String, resource_version: count, setup_revision: count,
  etag: Schema.String.check(Schema.isPattern(/^"[0-9a-f]{64}"$/u)),
  context: Schema.Struct({
    task_id: identity, generation: count, stop_generation: count, state,
    accepted_revision_id: identity, objective: Schema.String, goal_digest: Schema.String,
    questions: Schema.Array(question), covered_input_sequence: count,
    effect_cursor: count, confirmed_effect_watermark: count, checkpoint_id: Schema.NullOr(identity), expires_at: instant,
    limits: Schema.Struct({model_nanos: nanosSchema, wall_seconds: count, running_seconds: count,
      leases: count, publications: count, decision_calls: count, fallback_reads: count}),
    usage: Schema.Struct({model_nanos: nanosSchema, reserved_model_nanos: nanosSchema, running_nanos: count,
      leases: count, publications: count, decision_calls: count, fallback_reads: count}),
  }),
});
export type TaskDetail = typeof TaskDetailSchema.Type;

export function decodeTask<T>(schema: Schema.Codec<T, unknown>, value: unknown): Effect.Effect<T, ServerError> {
  return Schema.decodeUnknownEffect(schema)(value).pipe(Effect.mapError(() => new ServerError({
    detail: "The task response did not match the supported API shape",
    suggestion: "update the CLI and verify that it targets the matching daemon build",
    code: "TASK_RESPONSE_INVALID", status: 502, requestId: null,
  })));
}
