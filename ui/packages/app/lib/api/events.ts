import { request, requestWithRetry } from "./client";

import type { RetryOptions } from "./retry";
import { buildQuery } from "./events-types";
import type { EventsQuery, FRAME_KIND } from "./events-types";

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

/**
 * Where the fleet's counters stand after a frame — absolute, so the client
 * ASSIGNS them rather than adding to them. A snapshot is idempotent where a
 * delta is not: a dropped, duplicated or late frame cannot leave a tile wrong.
 *
 * Both or neither. The daemon flattens an optional pair, and a read that did
 * not answer sends the frame without them, which the store reads as "leave
 * what you have standing" — never zeros, which would say the fleet has done
 * nothing. Optional on the TYPE because the wire is untrusted.
 */
export type FleetCountersSnapshot = {
  events_processed?: number;
  budget_used_nanos?: number;
};

export type ActivityLiveFrame =
  // The row as the lease verb opened it: who raised it, how it entered, and
  // the row's own instant. Every field past the identifier is optional on the
  // TYPE because the wire is untrusted; the reducer guards each and falls back.
  // The counters ride here because a receive is when `events_processed` moves.
  | ({
      kind: typeof FRAME_KIND.EVENT_RECEIVED;
      event_id: string;
      actor: string;
      event_type?: EventTypeValue;
      created_at?: number;
    } & FleetCountersSnapshot)

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
  // The counters sit on this fleet-facts block, never on `EventRow`: the row
  // carries one event's figures and the snapshot carries the fleet's totals.
  | ({ kind: typeof FRAME_KIND.EVENT_COMPLETE; event_id: string } & Partial<
      Omit<EventRow, "event_id" | "fleet_id" | "workspace_id">
    > & {
        fleet_status?: string;
        pending_approvals?: number;
      } & FleetCountersSnapshot)
  // A human has been asked about one of the fleet's actions; the count is how
  // many answers are owed, this one included.
  | ({
      kind: typeof FRAME_KIND.GATE_OPENED;
      gate_id: string;
      event_id: string;
      pending_approvals: number;
    } & FleetCountersSnapshot)

  // A human answered, or the window closed with no answer. The event is null
  // for a gate raised outside a run (a standing grant).
  | ({
      kind: typeof FRAME_KIND.GATE_RESOLVED;
      gate_id: string;
      event_id: string | null;
      status: string;
      resolved_by: string;
      pending_approvals: number;
    } & FleetCountersSnapshot)

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
  /**
   * Where each announced fleet stands, keyed by fleet id, so a subscriber is
   * right before its first event frame. May be shorter than `fleet_ids`: a
   * read that did not answer omits the fleet, and the store leaves what it
   * has standing for it.
   */
  counters?: Record<string, FleetCountersSnapshot>;
};

export type WorkspaceCatchingUpFrame = {
  kind: typeof FRAME_KIND.CATCHING_UP;
  dropped: number;
};

export type WorkspaceControlFrame = WorkspaceHelloFrame | WorkspaceCatchingUpFrame;

export type LiveFrame = ActivityLiveFrame | WorkspaceControlFrame;

// One multiplexed SSE frame from the workspace stream: a `LiveFrame` plus the
// `fleet_id` the backend spliced in, so the wall demultiplexes each frame to
// its tile. The backend guarantees the tag on every frame; a frame missing it
// is malformed and dropped by the client (never routed to a wrong tile).
export type WorkspaceLiveFrame = ActivityLiveFrame & { fleet_id: string };

export type WorkspaceFrame = WorkspaceLiveFrame | WorkspaceControlFrame;
