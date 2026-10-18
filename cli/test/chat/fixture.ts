// Real HTTP transport with bounded, per-test private fixture state.

import { Effect, Layer, Option, Redacted } from "effect";
import { CliConfig } from "../../src/services/config.ts";
import { HttpClient, httpClientLayer } from "../../src/services/http-client.ts";
import { createTransport } from "../../src/commands/chat/transport.ts";
import { DRAFT_STATE, MESSAGE_ROLE, MESSAGE_STATE, SCHEDULE_SYNC, type Draft, type Review, type Fleet, type Schedule } from "../../src/commands/chat/protocol.ts";

export const WORKSPACE = "0199a801-1000-7000-8000-000000000001";
export const DRAFT_ID = "0199a801-1000-7000-8000-000000000002";
export const CANDIDATE_ID = "0199a801-1000-7000-8000-000000000003";
export const FLEET_ID = "0199a801-1000-7000-8000-000000000004";
export const LIBRARY_ID = "0199a801-1000-7000-8000-000000000005";
export const REVISION_ID = "0199a801-1000-7000-8000-000000000006";
export const SCHEDULE_ID = "0199a801-1000-7000-8000-000000000007";
export const MODEL = "owner/authoring";
export const PRIVATE_TEXT = "a private deployment question";
export const TOKEN = Redacted.make("local-chat-test");
export const REVIEW: Review = {
  effective: {definition: {schema_version: 1, description: "Watch Argo deployments", instructions_md: "Inspect the deployment", config: {}},
    instance_name: "argo", runtime_model_ref: "owner/runtime"},
  requirements: {}, readiness: [], review_digest: "reviewed-setup", policy_semantics: "native-v1", observed_at: "1970-01-01T00:00:00.001Z",
};
export const DRAFT: Draft = {
  id: DRAFT_ID, state: DRAFT_STATE.editing, title: "Argo", etag: '"draft-1"', authoring_model_ref: MODEL,
  cancelled_through: "0", candidate: {id: CANDIDATE_ID, revision: "1", definition: REVIEW.effective.definition,
    canonical_digest: "definition-1", policy_semantics: REVIEW.policy_semantics}, review: REVIEW,
};
export const FLEET: Fleet = {
  id: FLEET_ID, name: "argo", status: "active", etag: '"fleet-1"', setup_etag: '"setup-1"', setup_revision: "1",
  setup: {state: "ready", checks: []}, start: {kind: "none"}, schedules: [],
};
export const SYNCED_SCHEDULE: Schedule = {schedule_id: SCHEDULE_ID, fleet_id: FLEET_ID, sync: SCHEDULE_SYNC.synced};
export const FAILED_SCHEDULE: Schedule = {...SYNCED_SCHEDULE, sync: SCHEDULE_SYNC.failed, last_error: "scheduler refused the push"};
// A claim clears `last_error`, so a row the daemon stopped waiting on carries none.
export const SYNCING_SCHEDULE: Schedule = {...SYNCED_SCHEDULE, sync: SCHEDULE_SYNC.syncing, last_error: null};
// A status a newer daemon may add; the client must decode it and still treat the push as owed.
export const UNKNOWN_SYNC_SCHEDULE: Schedule = {...SYNCED_SCHEDULE, sync: "queued_upstream"};
// pin test: literal is the contract — the command a person copies to retry a failed schedule push.
export const SCHEDULE_RETRY = `agentsfleet schedule sync ${FLEET_ID} ${SCHEDULE_ID}`;
// pin test: literal is the contract — the diagnostic kinds a `--json` consumer branches on.
export const SYNC_FAILED_KIND = "schedule_sync_failed";
export const SYNC_UNCONFIRMED_KIND = "schedule_sync_unconfirmed";
// pin test: literal is the contract — the warning a terminal shows for a push nobody confirmed.
export const SCHEDULE_UNCONFIRMED =
  `Schedule ${SCHEDULE_ID} is saved but its registration was not confirmed, so it may not fire. Retry in a minute with: ${SCHEDULE_RETRY}`;
export const ACCEPTED = {status: "accepted", sequence: "1", replayed: false} as const;
export const MESSAGE = {sequence: "1", operation_id: DRAFT_ID, role: MESSAGE_ROLE.user,
  state: MESSAGE_STATE.completed, text: PRIVATE_TEXT, created_at: "1970-01-01T00:00:00.001Z"};
export const EMPTY_PAGE = {items: [], next_cursor: null};
export const PUBLICATION = {library_id: LIBRARY_ID, revision_id: REVISION_ID, head_revision_id: REVISION_ID, etag: '"library-1"'};
export const JSON_TYPE = "application/json";

export async function withApi<T>(handler: (request: Request) => Response | Promise<Response>,
  run: (transport: ReturnType<typeof createTransport>, origin: string) => Promise<T>): Promise<T> {
  const server = Bun.serve({hostname: "127.0.0.1", port: 0, fetch: handler});
  const origin = `http://127.0.0.1:${server.port}`;
  const config = Layer.succeed(CliConfig, {apiUrl: origin, dashboardUrl: origin, accessToken: Option.some(TOKEN),
    jsonMode: false, noOpen: true, telemetryPosthogKey: "", telemetryPosthogHost: origin});
  try {
    const http = await Effect.runPromise(HttpClient.pipe(Effect.provide(Layer.provide(httpClientLayer, config))));
    return await run(createTransport(http, WORKSPACE, TOKEN), origin);
  } finally { await server.stop(true); }
}

export function problem(status: number): Response {
  return Response.json({detail: "injected refusal", error_code: "CHAT_TEST_REFUSAL", title: "injected"}, {status});
}
