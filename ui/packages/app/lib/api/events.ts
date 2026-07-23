import { request } from "./client";
import { requestWithRetry, type RetryOptions } from "./retry";

// Operator-visible event rows from `core.fleet_events`. Mirrors the
// server's `EventRow` envelope verbatim (no shim, no rename) — the
// dashboard renders the same shape it queries.

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
  request_json: string;
  response_text: string | null;
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
    `/v1/workspaces/${workspaceId}/fleets/${fleetId}/events${buildQuery(opts)}`,
    { method: "GET" },
    token,
    retry,
  );
}

export async function listWorkspaceEvents(
  workspaceId: string,
  token: string,
  opts?: EventsQuery,
): Promise<EventsPage> {
  return request<EventsPage>(
    `/v1/workspaces/${workspaceId}/events${buildQuery(opts)}`,
    { method: "GET" },
    token,
  );
}

// Live progress frames published on `fleet:{id}:activity` (Redis pub/sub),
// fanned out to subscribers as SSE messages by the backend handler. The
// backend authoritatively shapes these — keep `FRAME_KIND` in sync with
// the KIND_* constants in src/agentsfleetd/fleet_runtime/activity_publisher.zig.
export const FRAME_KIND = {
  EVENT_RECEIVED: "event_received",
  TOOL_CALL_STARTED: "tool_call_started",
  TOOL_CALL_PROGRESS: "tool_call_progress",
  CHUNK: "chunk",
  TOOL_CALL_COMPLETED: "tool_call_completed",
  EVENT_COMPLETE: "event_complete",
  // Synthetic install-progression frames the create path emits on a deferred
  // tick post-201. The InstallStates surface advances its rendered step off
  // these; `install:ready` is the signal the fleet has flipped installing→active
  // on the server. Mirror the KIND_INSTALL_* constants in
  // src/agentsfleetd/fleet_runtime/activity_publisher.zig verbatim.
  INSTALL_CREATING: "install:creating",
  INSTALL_PROVISIONING: "install:provisioning",
  INSTALL_READY: "install:ready",
  INSTALL_ERROR: "install:error",
  HELLO: "hello",
  CATCHING_UP: "catching_up",
} as const;

export type FrameKind = (typeof FRAME_KIND)[keyof typeof FRAME_KIND];

export type ActivityLiveFrame =
  | { kind: typeof FRAME_KIND.EVENT_RECEIVED; event_id: string; actor: string }
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
  // `status` is optional — the backend can emit a status-less completion
  // frame, which the timeline resolves to "processed".
  // Empty-string failure fields mean "no failure cause" (the publisher always
  // includes them; a processed completion carries them empty).
  | {
      kind: typeof FRAME_KIND.EVENT_COMPLETE;
      event_id: string;
      status?: string;
      failure_label?: string;
      failure_detail?: string;
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
