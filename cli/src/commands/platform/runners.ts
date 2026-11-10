import { Effect } from "effect";
import { HTTP_METHOD } from "../../constants/http-method.ts";
import { Output, OUTPUT_FORMAT } from "../../services/output.ts";
import { ADMIN_RUNNERS_PATH, decodePlatform, invalidInput, platformInput, platformPath, platformQuery, platformRequest, platformResult, readPlatformFile } from "./common.ts";
import { RunnerDetailSchema, RunnerEnrolledSchema, RunnerEventsSchema, RunnerLabelsSchema, RunnerLeasesSchema, RunnerMutationSchema, RunnerPageSchema, RunnerPolicySchema, RunnerRotatedSchema } from "./runner-schema.ts";
import { withRunnerTokenFile } from "./token.ts";

export type RunnerPaging = {readonly limit?: number | undefined; readonly startingAfter?: string | undefined};
type RunnerRead = RunnerPaging & {readonly workspace?: string | undefined; readonly fleet?: string | undefined; readonly eventType?: string | undefined; readonly since?: string | undefined; readonly until?: string | undefined};
type RunnerEdit = {readonly policy?: string | undefined; readonly labels?: string | undefined};

export const platformRunnersList = Effect.fn("platform.runners.list")(function* (options: RunnerPaging) {
  const path = ADMIN_RUNNERS_PATH + platformQuery({limit: options.limit, starting_after: options.startingAfter});
  const page = yield* decodePlatform(RunnerPageSchema, yield* platformRequest({path}));
  const output = yield* Output;
  if (output.format === OUTPUT_FORMAT.json) return yield* output.printJson(page);
  yield* output.printEntityTable({name: {key: "host_id", label: "HOST"}, domain: [
    {key: "id", label: "ID"}, {key: "status", label: "STATE"}, {key: "liveness", label: "CONNECTION"},
  ], ageKey: null}, page.items.map(({id, host_id, status, liveness}) => ({id, host_id, status, liveness})));
  if (page.next_cursor) yield* output.info(`Next page: --starting-after ${page.next_cursor}`);
});

export const platformRunnerShow = Effect.fn("platform.runners.show")(function* (id: string) {
  yield* platformResult(yield* decodePlatform(RunnerDetailSchema, yield* platformRequest({path: platformPath(ADMIN_RUNNERS_PATH, id)})));
});

export const platformRunnerHistory = Effect.fn("platform.runners.history")(function* (id: string, kind: "events" | "leases", options: RunnerRead) {
  const query = platformQuery({limit: options.limit, starting_after: options.startingAfter, workspace_id: options.workspace,
    fleet: options.fleet, event_type: options.eventType, since: options.since, until: options.until});
  const body = yield* platformRequest({path: platformPath(ADMIN_RUNNERS_PATH, id, `/${kind}`) + query});
  if (kind === "events") yield* platformResult(yield* decodePlatform(RunnerEventsSchema, body));
  else yield* platformResult(yield* decodePlatform(RunnerLeasesSchema, body));
});

export const platformRunnerUpdate = Effect.fn("platform.runners.update")(function* (id: string, edit: RunnerEdit) {
  if (edit.policy === undefined && edit.labels === undefined) return yield* Effect.fail(invalidInput("Supply --policy or --labels"));
  const body = yield* runnerEdit(edit);
  yield* platformResult(yield* decodePlatform(RunnerMutationSchema, yield* platformRequest({
    path: platformPath(ADMIN_RUNNERS_PATH, id), method: HTTP_METHOD.patch, body,
  })));
});

export const platformRunnerAction = Effect.fn("platform.runners.action")(function* (id: string, action: string) {
  const response = yield* platformRequest({path: platformPath(ADMIN_RUNNERS_PATH, id), method: HTTP_METHOD.patch, body: {action}});
  yield* platformResult(yield* decodePlatform(RunnerMutationSchema, response));
});

export const platformRunnerDelete = Effect.fn("platform.runners.delete")(function* (id: string) {
  yield* platformRequest({path: platformPath(ADMIN_RUNNERS_PATH, id), method: HTTP_METHOD.delete});
  yield* platformResult({id, deleted: true});
});

export const platformRunnerEnroll = Effect.fn("platform.runners.enroll")(function* (host: string, edit: RunnerEdit & {readonly policy: string}, tokenFile: string) {
  const body = {host_id: host, labels: [], ...yield* runnerEdit(edit)};
  return yield* withRunnerTokenFile(tokenFile, (save) => Effect.gen(function* () {
    const enrolled = yield* decodePlatform(RunnerEnrolledSchema, yield* platformRequest({path: ADMIN_RUNNERS_PATH, method: HTTP_METHOD.post, body, retry: null}));
    yield* save(enrolled.runner_token);
    yield* platformResult({runner_id: enrolled.runner_id, assigned_policy: enrolled.assigned_policy, token_file: tokenFile});
  }));
});

export const platformRunnerRotate = Effect.fn("platform.runners.rotate")(function* (id: string, tokenFile: string) {
  return yield* withRunnerTokenFile(tokenFile, (save) => Effect.gen(function* () {
    const rotated = yield* decodePlatform(RunnerRotatedSchema, yield* platformRequest({
      path: platformPath(ADMIN_RUNNERS_PATH, id), method: HTTP_METHOD.patch, body: {action: "rotate"}, retry: null,
    }));
    yield* save(rotated.runner_token);
    yield* platformResult({id: rotated.id, token_file: tokenFile});
  }));
});

const runnerEdit = Effect.fn("platform.runners.editInput")(function* (edit: RunnerEdit) {
  const assigned_policy = edit.policy === undefined ? undefined : yield* platformInput(RunnerPolicySchema, yield* readPlatformFile(edit.policy));
  const labels = edit.labels === undefined ? undefined : yield* platformInput(RunnerLabelsSchema,
    [...new Set(edit.labels.split(",").map((label) => label.trim()).filter(Boolean))]);
  return {...assigned_policy === undefined ? {} : {assigned_policy}, ...labels === undefined ? {} : {labels}};
});
