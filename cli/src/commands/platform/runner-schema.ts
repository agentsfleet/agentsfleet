import { Schema } from "effect";
import {CordonReason, BindMode, RunDestination, RunKind, RunOutcome, RunnerEventType, RunnerLiveness, RunnerNetworkPolicy, RunnerState, SandboxTier} from "./runner-vocabulary.ts";
import { instant } from "../../lib/instant.ts";

const text = Schema.String.check(Schema.isMinLength(1));
const count = Schema.Number.check(Schema.isInt(), Schema.isGreaterThanOrEqualTo(0));
const label = text.check(Schema.isMaxLength(64), Schema.isPattern(/^[A-Za-z0-9_.-]+$/u));
export const RunnerLabelsSchema = Schema.Array(label).check(Schema.isMaxLength(32));
export const RunnerPolicySchema = Schema.Struct({
  sandbox_tier: Schema.Enum(SandboxTier),
  network_policy: Schema.Enum(RunnerNetworkPolicy),
  registry_allowlist: Schema.Array(text),
  worker_count: count.check(Schema.isBetween({minimum: 1, maximum: 64})),
  extra_binds: Schema.Array(Schema.Struct({path: text, mode: Schema.Enum(BindMode), note: Schema.String})),
  durable_tasks_enabled: Schema.Boolean,
  decision_model_id: Schema.optionalKey(Schema.NullOr(text)),
});
const status = Schema.Enum(RunnerState);
const capability = Schema.Struct({
  landlock: Schema.Boolean, seccomp: Schema.Boolean, bubblewrap: Schema.Boolean,
  cgroup_controllers: Schema.Array(Schema.String), egress_enforcement: Schema.Boolean,
});
const runner = Schema.Struct({
  id: text, host_id: text, sandbox_tier: Schema.Enum(SandboxTier), status,
  liveness: Schema.Enum(RunnerLiveness),
  cordon: Schema.optionalKey(Schema.Struct({reason: Schema.Enum(CordonReason), cordoned_at: instant})),
  labels: Schema.Array(Schema.String), last_seen_at: instant, created_at: instant,
  assigned_policy: RunnerPolicySchema, achievable: Schema.NullOr(capability),
  degraded: Schema.Boolean, degraded_reason: Schema.NullOr(Schema.String),
  /** The tenant that registered the runner; absent for a platform runner. */
  tenant_id: Schema.optionalKey(Schema.NullOr(text)),
});
export const RunnerPageSchema = Schema.Struct({
  items: Schema.Array(runner), total: Schema.NullOr(count), next_cursor: Schema.NullOr(text),
});
const selftest = Schema.Struct({
  checks: Schema.Array(Schema.Struct({name: Schema.String, ok: Schema.Boolean, detail: Schema.String})),
  all_ok: Schema.Boolean, sandbox_tier: Schema.Enum(SandboxTier), network_policy: Schema.Enum(RunnerNetworkPolicy),
});
export const RunnerDetailSchema = Schema.Struct({...runner.fields,
  active_run_count: count, active_fleet_count: count, runs_acquired: count,
  runs_succeeded: count, runs_failed: count, runs_expired: count,
  selftest_requested_at: Schema.NullOr(instant), selftest_completed_at: Schema.NullOr(instant), selftest: Schema.NullOr(selftest),
});
export const RunnerMutationSchema = Schema.Struct({
  id: text, status, assigned_policy: Schema.optionalKey(Schema.NullOr(RunnerPolicySchema)),
  labels: Schema.optionalKey(Schema.NullOr(Schema.Array(Schema.String))),
  selftest_requested_at: Schema.optionalKey(Schema.NullOr(instant)),
});
export const RunnerRegisteredSchema = Schema.Struct({runner_id: text, runner_token: text, assigned_policy: RunnerPolicySchema});
export const RunnerRotatedSchema = Schema.Struct({id: text, runner_token: text});
/** A tenant registration retried with its first key and body: the runner, never its token. */
export const RegistrationReplaySchema = Schema.Struct({runner_id: text, assigned_policy: RunnerPolicySchema});
export const RunnerEventsSchema = Schema.Struct({
  items: Schema.Array(Schema.Struct({id: text, runner_id: text, event_type: Schema.Enum(RunnerEventType), occurred_at: instant, metadata: Schema.Unknown})),
  total: Schema.NullOr(count), next_cursor: Schema.NullOr(text),
});
export const RunnerRunsSchema = Schema.Struct({
  items: Schema.Array(Schema.Struct({
    id: text, fleet_id: text, fleet_name: Schema.NullOr(Schema.String), workspace_id: text,
    destination: Schema.Enum(RunDestination), event_id: text, event_type: Schema.NullOr(Schema.String), actor: Schema.NullOr(Schema.String),
    outcome: Schema.Enum(RunOutcome), kind: Schema.Enum(RunKind), failure_label: Schema.NullOr(Schema.String), failure_detail: Schema.NullOr(Schema.String),
    fencing_token: count, provider: text, model: text, posture: text,
    metered_input_tokens: count, metered_cached_tokens: count, metered_output_tokens: count,
    wall_ms: Schema.NullOr(count), run_expires_at: instant, created_at: instant,
  })), total: Schema.NullOr(count), next_cursor: Schema.NullOr(text),
});
