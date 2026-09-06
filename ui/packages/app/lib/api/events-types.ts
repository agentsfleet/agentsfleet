// The live-frame vocabulary and the same-origin stream URLs the streaming layer shares. Dependency-free on purpose: client components read these
// without pulling the transport, whose retry policy is server-only.

export type EventsQuery = {
  cursor?: string;
  actor?: string;
  // Prefix filter on the event actor — the server matches `actor LIKE '<prefix>%'`
  // (events.zig). Mutually exclusive with `actor`; the server 400s if both are
  // sent. Onboarding uses `actor_prefix=steer:` to detect the first steer.
  actor_prefix?: string;
  since?: string;
  fleet_id?: string;
  limit?: number;
};

export function buildQuery(opts?: EventsQuery): string {
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

// Live frames published on `fleet:{id}:activity` (Redis pub/sub), fanned out
// as SSE messages by the backend handler. `hello` and `catching_up` are
// rustd/crates/afd_sse/src/frame.rs's. The four mid-run frames are the
// `Published` enum in rustd/crates/afd_fleet/src/lease/activity.rs, where the
// runner's wire vocabulary becomes this one. The two brackets and the two gate
// frames are the daemon's own `TailFrame` in rustd/crates/afd_wire/src/tail.rs:
// `event_received` when the lease opens the row, `event_complete` with the
// whole row when a report or a refusal closes it, `gate_opened` and
// `gate_resolved` when a human is asked and answers. Keep every spelling in
// sync with those enums; the install frames below are declared nowhere on the
// server — the SSE layer reads a payload's leading `kind` and forwards it.
export const FRAME_KIND = {
  EVENT_RECEIVED: "event_received",
  TOOL_CALL_STARTED: "tool_call_started",
  TOOL_CALL_PROGRESS: "tool_call_progress",
  CHUNK: "chunk",
  TOOL_CALL_COMPLETED: "tool_call_completed",
  EVENT_COMPLETE: "event_complete",
  GATE_OPENED: "gate_opened",
  GATE_RESOLVED: "gate_resolved",
  // Synthetic install-progression frames. The retired daemon published these
  // from a thread that slept after the 201 and then flipped
  // installing→active; the current install does that flip inside its own
  // pipeline and emits no `install:*` frame at all
  // (rustd/crates/afd_fleet_lifecycle/src/install.rs). The InstallStates
  // surface still advances its rendered step off these names, so they stay
  // here as the client's own vocabulary rather than a mirror of a server
  // constant — nothing on the server declares them.
  INSTALL_CREATING: "install:creating",
  INSTALL_PROVISIONING: "install:provisioning",
  INSTALL_READY: "install:ready",
  INSTALL_ERROR: "install:error",
  HELLO: "hello",
  CATCHING_UP: "catching_up",
} as const;

export type FrameKind = (typeof FRAME_KIND)[keyof typeof FRAME_KIND];

// Same-origin URL for the SSE stream. The path is intercepted by the
// Next Route Handler at app/live/.../events/stream/route.ts which
// injects the api-audience Bearer token server-side.
export function streamFleetEventsUrl(workspaceId: string, fleetId: string): string {
  return (
    `/live/v1/workspaces/${encodeURIComponent(workspaceId)}` +
    `/fleets/${encodeURIComponent(fleetId)}/events/stream`
  );
}

// Same-origin URL for the reconnect backfill list. Intercepted by the Next
// Route Handler at app/live/.../events/route.ts (the non-stream sibling
// of streamFleetEventsUrl's handler), which injects the Bearer token
// server-side. The opts type carries exactly the keys that handler forwards
// upstream — anything wider would be silently dropped at the proxy.
export function backfillFleetEventsUrl(
  workspaceId: string,
  fleetId: string,
  opts?: Pick<EventsQuery, "cursor" | "since" | "limit">,
): string {
  return (
    `/live/v1/workspaces/${encodeURIComponent(workspaceId)}` +
    `/fleets/${encodeURIComponent(fleetId)}/events${buildQuery(opts)}`
  );
}

// Same-origin URL for the ONE multiplexed workspace SSE stream. Intercepted by
// the Next Route Handler at app/live/.../events/stream/route.ts, which mints
// the api-audience Bearer server-side. This is the wall's single connection —
// it replaces the per-tile streamFleetEventsUrl fan-out.
export function streamWorkspaceEventsUrl(workspaceId: string): string {
  return `/live/v1/workspaces/${encodeURIComponent(workspaceId)}/events/stream`;
}

// Same-origin URL for the workspace-scoped reconnect backfill list. The wall
// recovers a gap by paging `core.fleet_events` for the whole workspace (or one
// fleet via `fleet_id`), the same durable source the per-fleet client uses.
export function backfillWorkspaceEventsUrl(
  workspaceId: string,
  opts?: Pick<EventsQuery, "cursor" | "since" | "limit" | "fleet_id">,
): string {
  return `/live/v1/workspaces/${encodeURIComponent(workspaceId)}/events${buildQuery(opts)}`;
}
