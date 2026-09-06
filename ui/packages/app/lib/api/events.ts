import { request, requestWithRetry } from "./client";
import type { RetryOptions } from "./retry";

// The fleet-scoped `/v1` routes hang off this segment; the ids are encoded
// here, once, so a caller-controlled value carrying `/`, `?` or `#` can never
// re-target a server-held-token request at another route or query. The
// `/live/` builders below encode inline for the same reason.
function fleetScope(workspaceId: string, fleetId: string): string {
  return `/v1/workspaces/${encodeURIComponent(workspaceId)}/fleets/${encodeURIComponent(fleetId)}`;
}


// Operator-visible event rows from `core.fleet_events`. Mirrors the
// server's `EventRow` envelope verbatim (no shim, no rename) — the
// dashboard renders the same shape it queries.
//
// No request or response body: a page is up to 200 rows, and the list read is
// deliberately kept off oversized-attribute storage. Anything that needs a body
// reads one event through `getFleetEvent` below.

export type EventStatus = "received" | "processed" | "fleet_error" | "gate_blocked";
export type EventType = "chat" | "webhook" | "cron" | "continuation";
export type EventStatusValue = EventStatus | (string & {});
export type EventTypeValue = EventType | (string & {});

export type EventRow = {
  event_id: string;
  fleet_id: string;
  workspace_id: string;
  actor: string;
  event_type: EventTypeValue;
  status: EventStatusValue;
  tokens: number | null;
  wall_ms: number | null;
  failure_label: string | null;
  /**
   * Human-readable cause line from the runner's classification site (which
   * check failed, and why). `null` on success or when an older runner omitted
   * it — every surface then falls back to the canned `failure_label` sentence.
   */
  failure_detail: string | null;
  checkpoint_id: string | null;
  resumes_event_id: string | null;
  /**
   * Summed telemetry `credit_deducted_nanos` for this event — server truth
   * (M131 §2). `null` when the event recorded no telemetry: the ledger renders
   * that as unknown (`—`), never a fabricated zero, and never derives cost from
   * `tokens`.
   */
  cost_nanos: number | null;
  /** epoch milliseconds */
  created_at: number;
  /** epoch milliseconds */
  updated_at: number;
};

export type EventsPage = {
  items: EventRow[];
  next_cursor: string | null;
};

/**
 * One event with everything recorded about it — the list row plus the two
 * bodies the list read omits. Fetched when a row is expanded, never for a page:
 * a page is up to 200 rows and would carry every payload to render a table.
 */
export type EventDetail = EventRow & {
  /** The trigger payload as stored, serialized to JSON text. */
  request_json: string;
  /**
   * The agent's full answer. `null` while a run is in flight, and on a run that
   * failed before producing one.
   */
  response_text: string | null;
};

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

function buildQuery(opts?: EventsQuery): string {
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

export async function listFleetEvents(
  workspaceId: string,
  fleetId: string,
  token: string,
  opts?: Omit<EventsQuery, "fleet_id">,
  retry?: RetryOptions,
): Promise<EventsPage> {
  return requestWithRetry<EventsPage>(
    `${fleetScope(workspaceId, fleetId)}/events${buildQuery(opts)}`,
    { method: "GET" },
    token,
    retry,
  );
}

export type ThreadPage = {
  items: EventDetail[];
  total: null;
  next_cursor: string | null;
};

export type ThreadQuery = {
  starting_after?: string;
  /** Server default 20, max 25 — thread rows carry full bodies. */
  limit?: number;
};

/**
 * The chat thread, bodies included, one request — replaces reading the event
 * list and then one detail per turn. Newest first; pages are byte-budgeted on
 * the server (the newest turn always ships) and `next_cursor` continues a
 * long thread.
 */
export async function listFleetMessages(
  workspaceId: string,
  fleetId: string,
  token: string,
  opts?: ThreadQuery,
  retry?: RetryOptions,
): Promise<ThreadPage> {
  const params = new URLSearchParams();
  if (opts?.starting_after) params.set("starting_after", opts.starting_after);
  if (opts?.limit != null) params.set("limit", String(opts.limit));
  const qs = params.toString();
  return requestWithRetry<ThreadPage>(
    `${fleetScope(workspaceId, fleetId)}/messages${qs.length > 0 ? `?${qs}` : ""}`,
    { method: "GET" },
    token,
    retry,
  );
}

/**
 * Read one event's full record. A 404 covers both an unknown identifier and an
 * event in another workspace — the server does not distinguish them, so neither
 * can the caller.
 */
export async function getFleetEvent(
  workspaceId: string,
  fleetId: string,
  eventId: string,
  token: string,
): Promise<EventDetail> {
  return request<EventDetail>(
    `${fleetScope(workspaceId, fleetId)}/events/${encodeURIComponent(eventId)}`,
    { method: "GET" },
    token,
  );
}

export async function listWorkspaceEvents(
  workspaceId: string,
  token: string,
  opts?: EventsQuery,
): Promise<EventsPage> {
  return request<EventsPage>(
    `/v1/workspaces/${encodeURIComponent(workspaceId)}/events${buildQuery(opts)}`,
    { method: "GET" },
    token,
  );
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

export type ActivityLiveFrame =
  // The row as the lease verb opened it: who raised it, how it entered, and
  // the row's own instant. Every field past the identifier is optional on the
  // TYPE because the wire is untrusted; the reducer guards each and falls back.
  | {
      kind: typeof FRAME_KIND.EVENT_RECEIVED;
      event_id: string;
      actor: string;
      event_type?: EventTypeValue;
      created_at?: number;
    }
  | {
      kind: typeof FRAME_KIND.TOOL_CALL_STARTED;
      event_id: string;
      name: string;
      args_redacted: unknown;
    }
  | {
      kind: typeof FRAME_KIND.TOOL_CALL_PROGRESS;
      event_id: string;
      name: string;
      elapsed_ms: number;
    }
  | { kind: typeof FRAME_KIND.CHUNK; event_id: string; text: string }
  | {
      kind: typeof FRAME_KIND.TOOL_CALL_COMPLETED;
      event_id: string;
      name: string;
      ms: number;
    }
  // The terminal row as the events list serves it, less the two scope columns
  // the channel names, plus the two fleet facts a run can change — a watcher
  // folds this in and reads nothing. Typed partial past the identifier for
  // the reason the opening bracket is.
  | ({ kind: typeof FRAME_KIND.EVENT_COMPLETE; event_id: string } & Partial<
      Omit<EventRow, "event_id" | "fleet_id" | "workspace_id">
    > & {
        fleet_status?: string;
        pending_approvals?: number;
      })
  // A human has been asked about one of the fleet's actions; the count is how
  // many answers are owed, this one included.
  | {
      kind: typeof FRAME_KIND.GATE_OPENED;
      gate_id: string;
      event_id: string;
      pending_approvals: number;
    }
  // A human answered, or the window closed with no answer. The event is null
  // for a gate raised outside a run (a standing grant).
  | {
      kind: typeof FRAME_KIND.GATE_RESOLVED;
      gate_id: string;
      event_id: string | null;
      status: string;
      resolved_by: string;
      pending_approvals: number;
    }
  // Install-progression frames carry only their discriminating `kind` (the kind
  // itself names the step). The registry forks these off the chat-event path —
  // they advance the install step, never the message list.
  | { kind: typeof FRAME_KIND.INSTALL_CREATING }
  | { kind: typeof FRAME_KIND.INSTALL_PROVISIONING }
  | { kind: typeof FRAME_KIND.INSTALL_READY }
  | { kind: typeof FRAME_KIND.INSTALL_ERROR };

export type WorkspaceHelloFrame = {
  kind: typeof FRAME_KIND.HELLO;
  fleet_ids: string[];
};

export type WorkspaceCatchingUpFrame = {
  kind: typeof FRAME_KIND.CATCHING_UP;
  dropped: number;
};

export type WorkspaceControlFrame = WorkspaceHelloFrame | WorkspaceCatchingUpFrame;
export type LiveFrame = ActivityLiveFrame | WorkspaceControlFrame;

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

// One multiplexed SSE frame from the workspace stream: a `LiveFrame` plus the
// `fleet_id` the backend spliced in, so the wall demultiplexes each frame to
// its tile. The backend guarantees the tag on every frame; a frame missing it
// is malformed and dropped by the client (never routed to a wrong tile).
export type WorkspaceLiveFrame = ActivityLiveFrame & { fleet_id: string };
export type WorkspaceFrame = WorkspaceLiveFrame | WorkspaceControlFrame;

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
