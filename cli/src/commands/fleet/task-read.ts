// Native task responses are validated before text or JSON success is emitted.

import { Effect, Schema } from "effect";
import { ServerError } from "../../errors/index.ts";
import { instant } from "../../lib/instant.ts";
import { isValidId } from "../../lib/id.ts";
import { nanosSchema } from "../../lib/nanos.ts";

const identity = Schema.String.check(Schema.makeFilter(isValidId));
const count = Schema.Number.check(Schema.makeFilter((value: number) => Number.isSafeInteger(value) && value >= 0));
const state = Schema.Literals(["ready", "running", "waiting", "blocked", "paused", "done", "cancelled", "exhausted"]);
const oid = Schema.String.check(Schema.isPattern(/^[0-9a-f]{40}$/u));
const target = Schema.Union([
  Schema.Struct({kind: Schema.Literal("api_only")}),
  Schema.Struct({kind: Schema.Literal("existing_pr"), repository: Schema.String, number: count, branch: Schema.String,
    head: oid, tree: oid, url: Schema.String, workflows: Schema.Array(Schema.Struct({id: count, path: Schema.String, digest: Schema.String}))}),
]);
const dependency = Schema.Struct({active: Schema.Boolean, head: oid, updated_at: instant, checks: Schema.Array(Schema.Struct({
  workflow_id: count, run_id: Schema.NullOr(count), run_number: Schema.NullOr(count), run_attempt: Schema.NullOr(count),
  status: Schema.String, conclusion: Schema.NullOr(Schema.String),
}))});
const question = Schema.Struct({name: Schema.String, question: Schema.Struct({
  type: Schema.Literal("noul"), instructions: Schema.String,
  criteria: Schema.Struct({true: Schema.String, false: Schema.String}),
})});

export const TaskDetailSchema = Schema.Struct({
  publication: Schema.optionalKey(Schema.NullOr(Schema.Struct({effect_id: identity, ordinal: count,
    state: Schema.Literals(["prepared", "issued", "confirmed", "unknown", "refused"]),
    expected_head: oid, tested_tree: oid, commit: Schema.NullOr(oid), pull_request_url: Schema.String,
  }))),
  dependency: Schema.optionalKey(Schema.NullOr(dependency)),
  title: Schema.String, resource_version: count, setup_revision: count,
  etag: Schema.String.check(Schema.isPattern(/^"[0-9a-f]{64}"$/u)),
  context: Schema.Struct({
    target: Schema.optionalKey(target),
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
