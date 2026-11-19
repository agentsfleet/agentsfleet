import {RunnerAction, RunnerHistory} from "./runner-vocabulary.ts";
import { Clock, Effect, Schema } from "effect";
import {instantNanos} from "../../lib/instant.ts";
import { eventTime } from "../../lib/event-time.ts";
import { HTTP_METHOD } from "../../constants/http-method.ts";
import { Output, OUTPUT_FORMAT } from "../../services/output.ts";
import { ADMIN_RUNNERS_PATH, decodePlatform, invalidInput, platformInput, platformPath, platformQuery, platformRequest, platformResult, readPlatformFile } from "./common.ts";
import { RegistrationReplaySchema, RunnerDetailSchema, RunnerRegisteredSchema, RunnerEventsSchema, RunnerLabelsSchema, RunnerRunsSchema, RunnerMutationSchema, RunnerPageSchema, RunnerPolicySchema, RunnerRotatedSchema } from "./runner-schema.ts";
import { withRunnerTokenFile } from "./token.ts";
import type { RunnerKind } from "./runner-kind.ts";
import { v7 as uuidv7 } from "uuid";
import { ServerError } from "../../errors/index.ts";

export type RunnerPaging = {readonly limit?: number | undefined; readonly startingAfter?: string | undefined};
type RunnerRead = RunnerPaging & {readonly workspace?: string | undefined; readonly fleet?: string | undefined; readonly eventType?: string | undefined; readonly since?: string | undefined; readonly until?: string | undefined};
type RunnerEdit = {readonly policy?: string | undefined; readonly labels?: string | undefined};
const DEFAULT_ENGINE = "Bubblewrap";
/** Who owns a host, as the operator's views name it. */
const owner = (tenant: string | null | undefined): string => tenant ?? "Platform";
const isOperator = (kind: RunnerKind): boolean => kind.path === ADMIN_RUNNERS_PATH;

export const runnersList = Effect.fn("runners.list")(function* (kind: RunnerKind, options: RunnerPaging) {
  const path = kind.path + platformQuery({limit: options.limit, starting_after: options.startingAfter});
  const page = yield* decodePlatform(RunnerPageSchema, yield* platformRequest({path}));
  const output = yield* Output;
  if (output.format === OUTPUT_FORMAT.json || !output.stdoutIsTty) return yield* output.printJson(page);
  yield* output.printEntityTable({name: {key: "host_id", label: "HOST"}, domain: [
    {key: "id", label: "ID"}, ...(isOperator(kind) ? [{key: "owner", label: "OWNER"}] : []),
    {key: "status", label: "STATE"}, {key: "liveness", label: "CONNECTION"},
    {key: "cordon", label: "AUTO CORDON"}, {key: "default_engine", label: "DEFAULT ENGINE"},
  ], ageKey: null}, page.items.map(({id, host_id, status, liveness, tenant_id, cordon}) =>
    ({id, host_id, owner: owner(tenant_id), status, liveness, cordon: cordon?.reason ?? "None", default_engine: DEFAULT_ENGINE})));
  if (page.next_cursor) yield* output.info(`Next page: --starting-after ${page.next_cursor}`);
});

export const runnerShow = Effect.fn("runners.show")(function* (kind: RunnerKind, id: string) {
  const runner = yield* decodePlatform(RunnerDetailSchema, yield* platformRequest({path: platformPath(kind.path, id)}));
  yield* platformResult(runner, runner.host_id, {
    ID: runner.id, ...(isOperator(kind) ? {Owner: owner(runner.tenant_id)} : {}),
    State: runner.status, Connection: runner.liveness, "Default engine": DEFAULT_ENGINE,
    "Isolation tier": runner.assigned_policy.sandbox_tier,
    "Network policy": runner.assigned_policy.network_policy, Workers: String(runner.assigned_policy.worker_count),
    Labels: runner.labels.join(", ") || "None", "Last seen": runner.last_seen_at,
    Health: runner.degraded ? `Degraded: ${runner.degraded_reason ?? "unknown reason"}` : "Ready",
    "Active runs": String(runner.active_run_count), "Active fleets": String(runner.active_fleet_count),
    "Runs succeeded": String(runner.runs_succeeded), "Runs failed": String(runner.runs_failed),
    "Self-test": selftestStatus(runner),
    "Auto cordon": runner.cordon?.reason ?? "None",
    ...(runner.cordon ? {"Cordoned at": runner.cordon.cordoned_at,
      "Return rule": "30 seconds of continuous healthy heartbeats"} : {}),
  });
  if (runner.selftest) {
    const output = yield* Output;
    if (output.format === OUTPUT_FORMAT.text && output.stdoutIsTty) {
      for (const check of runner.selftest.checks) yield* output.info(`${check.ok ? "✓" : "✕"} ${check.name}: ${check.detail}`);
    }
  }
});

export const runnerHistory = Effect.fn("runners.history")(function* (id: string, kind: RunnerHistory, options: RunnerRead) {
  const now = yield* Clock.currentTimeMillis;
  const since = options.since === undefined ? undefined : eventTime(options.since, now, true);
  const until = options.until === undefined ? undefined : eventTime(options.until, now, false);
  if (since !== undefined && until !== undefined && Date.parse(since) > Date.parse(until)) {
    return yield* Effect.fail(invalidInput("--until must be at or after --since"));
  }
  const query = platformQuery({limit: options.limit, starting_after: options.startingAfter, workspace_id: options.workspace,
    fleet: options.fleet, event_type: options.eventType, since, until});
  const body = yield* platformRequest({path: platformPath(ADMIN_RUNNERS_PATH, id, `/${kind}`) + query});
  const output = yield* Output;
  if (kind === RunnerHistory.History) {
    const page = yield* decodePlatform(RunnerEventsSchema, body);
    if (output.format === OUTPUT_FORMAT.json || !output.stdoutIsTty) return yield* output.printJson(page);
    yield* output.printEntityTable({id: {key: "id", label: "ID"}, domain: [
      {key: "event_type", label: "EVENT"}, {key: "occurred_at", label: "OCCURRED"},
    ], ageKey: null}, page.items);
    if (page.next_cursor) yield* output.info(`Next page: --starting-after ${page.next_cursor}`);
  } else {
    const page = yield* decodePlatform(RunnerRunsSchema, body);
    if (output.format === OUTPUT_FORMAT.json || !output.stdoutIsTty) return yield* output.printJson(page);
    yield* output.printEntityTable({id: {key: "id", label: "ID"}, name: {key: "fleet", label: "FLEET"}, domain: [
      {key: "outcome", label: "OUTCOME"}, {key: "kind", label: "KIND"}, {key: "model", label: "MODEL"},
    ], ageKey: null}, page.items.map(row => ({...row, fleet: row.fleet_name ?? row.fleet_id})));
    if (page.next_cursor) yield* output.info(`Next page: --starting-after ${page.next_cursor}`);
  }
});

export const runnerUpdate = Effect.fn("runners.update")(function* (kind: RunnerKind, id: string, edit: RunnerEdit) {
  if (edit.policy === undefined && edit.labels === undefined) return yield* Effect.fail(invalidInput("Supply --policy or --labels"));
  const body = yield* runnerEdit(edit);
  const runner = yield* decodePlatform(RunnerMutationSchema, yield* platformRequest({
    path: platformPath(kind.path, id), method: HTTP_METHOD.patch, body,
  }));
  yield* platformResult(runner, "Runner updated", {ID: runner.id, State: runner.status});
});

export const runnerAction = Effect.fn("runners.action")(function* (kind: RunnerKind, id: string, action: RunnerAction) {
  const response = yield* platformRequest({path: platformPath(kind.path, id), method: HTTP_METHOD.patch, body: {action}});
  const runner = yield* decodePlatform(RunnerMutationSchema, response);
  yield* platformResult(runner, "Runner action requested", {ID: runner.id, Action: action, State: runner.status});
});

export const runnerDelete = Effect.fn("runners.delete")(function* (kind: RunnerKind, id: string) {
  yield* platformRequest({path: platformPath(kind.path, id), method: HTTP_METHOD.delete});
  yield* platformResult({id, deleted: true}, "Runner deleted", {ID: id});
});

const REPLAY_DETAIL = "This runner was registered earlier; its token was shown once and is never stored";
// The registration already happened; a named code lets a script tell this from a server failure.
const REPLAY_CODE = "RUNNER_REGISTRATION_REPLAYED";

export const runnerRegister = Effect.fn("runners.register")(function* (kind: RunnerKind, host: string, edit: RunnerEdit & {readonly policy: string}, tokenFile: string) {
  const body = {host_id: host, labels: [], ...yield* runnerEdit(edit)};
  // One key per invocation: the CLI's own retries resend it, so a lost reply
  // inside this run never registers twice. The platform route takes no key.
  const request = kind.idempotentRegister
    ? {path: kind.path, method: HTTP_METHOD.post, body, headers: {"Idempotency-Key": uuidv7()}}
    : {path: kind.path, method: HTTP_METHOD.post, body, retry: {maxAttempts: 1}};
  return yield* withRunnerTokenFile(tokenFile, (save) => Effect.gen(function* () {
    const answer = yield* platformRequest(request);
    // A token-carrying answer first: a struct schema ignores extra keys, so the
    // replay shape also matches a fresh registration.
    const fresh = Schema.decodeUnknownOption(RunnerRegisteredSchema)(answer);
    const replay = fresh._tag === "None" ? Schema.decodeUnknownOption(RegistrationReplaySchema)(answer) : fresh;
    if (fresh._tag === "None" && replay._tag === "Some") {
      return yield* Effect.fail(new ServerError({
        detail: REPLAY_DETAIL, code: REPLAY_CODE, status: 200, requestId: null,
        suggestion: `agentsfleet runners rotate-token ${replay.value.runner_id} --token-out <new-file>`,
      }));
    }
    const registered = yield* decodePlatform(RunnerRegisteredSchema, answer);
    yield* save(registered.runner_token);
    yield* platformResult({runner_id: registered.runner_id, assigned_policy: registered.assigned_policy, token_file: tokenFile}, "Runner registered", {ID: registered.runner_id, "Token file": tokenFile});
  }));
});

export const runnerRotate = Effect.fn("runners.rotate")(function* (kind: RunnerKind, id: string, tokenFile: string) {
  return yield* withRunnerTokenFile(tokenFile, (save) => Effect.gen(function* () {
    const rotated = yield* decodePlatform(RunnerRotatedSchema, yield* platformRequest({
      path: platformPath(kind.path, id), method: HTTP_METHOD.patch, body: {action: RunnerAction.Rotate}, retry: {maxAttempts: 1},
    }));
    yield* save(rotated.runner_token);
    yield* platformResult({id: rotated.id, token_file: tokenFile}, "Runner token rotated", {ID: rotated.id, "Token file": tokenFile});
  }));
});

const runnerEdit = Effect.fn("platform.runners.editInput")(function* (edit: RunnerEdit) {
  const assigned_policy = edit.policy === undefined ? undefined : yield* platformInput(RunnerPolicySchema, yield* readPlatformFile(edit.policy));
  const labels = edit.labels === undefined ? undefined : yield* platformInput(RunnerLabelsSchema,
    [...new Set(edit.labels.split(",").map((label) => label.trim()).filter(Boolean))]);
  return {...assigned_policy === undefined ? {} : {assigned_policy}, ...labels === undefined ? {} : {labels}};
});

function selftestStatus(runner: typeof RunnerDetailSchema.Type): string {
  const completed = runner.selftest_completed_at;
  const requested = runner.selftest_requested_at;
  if (requested !== null && (completed === null || instantNanos(requested) > instantNanos(completed))) return "Pending";
  if (!runner.selftest) return "Not reported";
  if (completed === null || runner.selftest.sandbox_tier !== runner.assigned_policy.sandbox_tier
    || runner.selftest.network_policy !== runner.assigned_policy.network_policy) return "Stale (previous assignment)";
  return runner.selftest.all_ok ? "Passed" : "Failed";
}
