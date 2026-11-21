// The live-frame vocabulary and the same-origin stream URLs the streaming layer shares. Dependency-free on purpose: client components read these
// without pulling the transport, whose retry policy is server-only.

export type TurnsQuery = {
  cursor?: string;
  actor?: string;
  // Prefix filter on the turn actor — the server matches `actor LIKE '<prefix>%'`
  // (`prefix_to_like` in rustd/crates/daemon/afd_turns/src/history/filter.rs).
  // Mutually exclusive with `actor`; the server 400s if both are sent
  // (`DETAIL_ACTOR_AMBIGUOUS` in
  // rustd/crates/daemon/afd_api_tenant/src/handler/turn/query.rs). Onboarding uses `actor_prefix=steer:` to detect the first steer.
  actor_prefix?: string;
  since?: string;
  fleet_id?: string;
  limit?: number;
};

export function buildQuery(opts?: TurnsQuery): string {
  if (!opts) return "";
  const params = new URLSearchParams();
  if (opts.cursor) params.set("cursor", opts.cursor);
  if (opts.actor) params.set("actor", opts.actor);
  if (opts.actor_prefix) params.set("actor_prefix", opts.actor_prefix);
  if (opts.since) params.set("since", opts.since);
  if (opts.fleet_id) params.set("fleet_id", opts.fleet_id);
  if (opts.limit != null) params.set("limit", String(opts.limit));
  const qs = params.toString();
  return qs.length > 0 ? `?${qs}` : "";
}

// Live frames published on the NATS subject `af.thread.{id}.items`, fanned out
// as SSE messages by the backend handler. `hello`, `catching_up` and
// `access_revoked` are rustd/crates/daemon/afd_sse/src/frame/mod.rs's `KIND_*` control
// frames. The four mid-run frames are the
// `Published` enum in rustd/crates/daemon/afd_fleet/src/run/activity/mod.rs, where the
// runner's wire vocabulary becomes this one. The two brackets and the two gate
// frames are the daemon's own `TailFrame` in rustd/crates/daemon/afd_api_wire/src/tail/mod.rs:
// `turn_admitted` when a person's message is accepted, before any runner has
// it, `turn_received` when the run opens the row, `turn_complete` with the
// whole row when a report or a refusal closes it, `gate_opened` and
// `gate_resolved` when a human is asked and answers. Keep every spelling in
// sync with those enums.
export const FRAME_KIND = {
  TASK_CHANGED: "task_changed",
  TASK_DECISION_STARTED: "task_decision_started",
  TASK_DECISION_COMPLETED: "task_decision_completed",
  TASK_TRANSFER_PROGRESS: "task_transfer_progress",
  TURN_ADMITTED: "turn_admitted",
  TURN_RECEIVED: "turn_received",
  TOOL_CALL_STARTED: "tool_call_started",
  TOOL_CALL_PROGRESS: "tool_call_progress",
  CHUNK: "chunk",
  RUN_RESTARTED: "run_restarted",
  CONTEXT_PROGRESS: "context_progress",
  TOOL_CALL_COMPLETED: "tool_call_completed",
  TURN_COMPLETE: "turn_complete",
  GATE_OPENED: "gate_opened",
  GATE_RESOLVED: "gate_resolved",
  HELLO: "hello",
  CATCHING_UP: "catching_up",
  // The last frame of a stream whose caller lost access to what it streams.
  // The daemon closes the stream after it and refuses the next request with
  // the same code, so a client stops rather than reconnects.
  ACCESS_REVOKED: "access_revoked",
} as const;

export type FrameKind = (typeof FRAME_KIND)[keyof typeof FRAME_KIND];

/** Every frame a stream hands to its readers. `access_revoked` is not one: it
 * ends the stream, so each registry listens for it on its own. */
export const ROUTED_FRAME_KINDS: readonly FrameKind[] = Object.values(FRAME_KIND).filter(
  (kind) => kind !== FRAME_KIND.ACCESS_REVOKED,
);

// Same-origin URL for the SSE stream. The path is intercepted by the
// Next Route Handler at app/live/.../turns/stream/route.ts which
// injects the api-audience Bearer token server-side.
export function streamFleetTurnsUrl(workspaceId: string, fleetId: string): string {
  return (
    `/live/v1/workspaces/${encodeURIComponent(workspaceId)}` +
    `/fleets/${encodeURIComponent(fleetId)}/turns/stream`
  );
}

// Same-origin URL for the reconnect backfill list. Intercepted by the Next
// Route Handler at app/live/.../turns/route.ts (the non-stream sibling
// of streamFleetTurnsUrl's handler), which injects the Bearer token
// server-side. The opts type carries exactly the keys that handler forwards
// upstream — anything wider would be silently dropped at the proxy.
export function backfillFleetTurnsUrl(
  workspaceId: string,
  fleetId: string,
  opts?: Pick<TurnsQuery, "cursor" | "since" | "limit">,
): string {
  return (
    `/live/v1/workspaces/${encodeURIComponent(workspaceId)}` +
    `/fleets/${encodeURIComponent(fleetId)}/turns${buildQuery(opts)}`
  );
}

// Same-origin URL for one turn's saved row, bodies included. Intercepted by
// the Route Handler at app/live/.../turns/[turnId]/route.ts, which injects
// the Bearer token server-side.
export function fleetTurnDetailUrl(workspaceId: string, fleetId: string, turnId: string): string {
  return (
    `/live/v1/workspaces/${encodeURIComponent(workspaceId)}` +
    `/fleets/${encodeURIComponent(fleetId)}/turns/${encodeURIComponent(turnId)}`
  );
}

// Same-origin URL for one tool call's full arguments and output. Intercepted by
// the Route Handler at app/live/.../turns/[turnId]/items/[itemId]/route.ts.
// A call id is `{fence}:{n}`, so it is encoded like every other segment.
export function fleetTurnItemUrl(workspaceId: string, fleetId: string, turnId: string, callId: string): string {
  return `${fleetTurnDetailUrl(workspaceId, fleetId, turnId)}/items/${encodeURIComponent(callId)}`;
}

// Same-origin URL for the ONE multiplexed workspace SSE stream. Intercepted by
// the Next Route Handler at app/live/.../turns/stream/route.ts, which mints
// the api-audience Bearer server-side. This is the wall's single connection —
// it replaces the per-tile streamFleetTurnsUrl fan-out.
export function streamWorkspaceTurnsUrl(workspaceId: string): string {
  return `/live/v1/workspaces/${encodeURIComponent(workspaceId)}/turns/stream`;
}

// Same-origin URL for the workspace-scoped reconnect backfill list. The wall
// recovers a gap by paging `thread.turns` for the whole workspace (or one
// fleet via `fleet_id`), the same durable source the per-fleet client uses.
export function backfillWorkspaceTurnsUrl(
  workspaceId: string,
  opts?: Pick<TurnsQuery, "cursor" | "since" | "limit" | "fleet_id">,
): string {
  return `/live/v1/workspaces/${encodeURIComponent(workspaceId)}/turns${buildQuery(opts)}`;
}
