import { request, requestWithRetry } from "./client";
import { TurnsPageSchema, type TurnRow, type TurnsPage } from "./event-history-schema";
export type { TurnRow, TurnsPage } from "./event-history-schema";

import type { RetryOptions } from "./retry";
import type { ContextProgress } from "../context-progress";
import { buildQuery } from "./events-types";
import type { TurnsQuery, FRAME_KIND } from "./events-types";

// The fleet-scoped `/v1` routes hang off this segment; the ids are encoded
// here, once, so a caller-controlled value carrying `/`, `?` or `#` can never
// re-target a server-held-token request at another route or query. The
// `/live/` builders below encode inline for the same reason.
function fleetScope(workspaceId: string, fleetId: string): string {
  return `/v1/workspaces/${encodeURIComponent(workspaceId)}/fleets/${encodeURIComponent(fleetId)}`;
}

// Operator-visible turn rows from `thread.turns`. Mirrors the
// server's `TurnRow` envelope verbatim (no shim, no rename) — the
// dashboard renders the same shape it queries.
//
// No request or response body: a page is up to 200 rows, and the list read is
// deliberately kept off oversized-attribute storage. Anything that needs a body
// reads one event through `getFleetTurn` below.

export type TurnStatus = "received" | "processed" | "fleet_error" | "gate_blocked";

export type EventType = "chat" | "webhook" | "cron" | "continuation";

export type TurnStatusValue = TurnStatus | (string & {});

export type EventTypeValue = EventType | (string & {});

/** One native event address; the server checks both workspace and fleet ownership. */
export type TurnLocator = Pick<TurnRow, "workspace_id" | "fleet_id" | "turn_id">;

/**
 * One event with everything recorded about it — the list row plus the two
 * bodies the list read omits. Fetched when a row is expanded, never for a page:
 * a page is up to 200 rows and would carry every payload to render a table.
 */
export type TurnDetail = TurnRow & {
  /** The trigger payload as stored, serialized to JSON text. */
  request_json: string;
  /**
   * The agent's full answer. `null` while a run is in flight, and on a run that
   * failed before producing one.
   */
  response_text: string | null;
  /**
   * The run's tool calls as the runner reported them at settle, so a reload
   * shows the same rows the live tail did. `null` when none were recorded.
   */
  tool_calls: SavedToolTrace | null;
  /** Bounded public stages retained independently of private recovery state. */
  task_activity?: unknown;
};

/** `af_wire::tool_trace::ToolTrace`: the calls in order, and how many the
 * runner dropped to stay inside the trace's bounds. */
export type SavedToolTrace = {
  calls: SavedToolCall[];
  omitted_call_count: number;
};

export type SavedToolCall = {
  /** Fenced, `{fence}:{n}` — the id the full-call read takes. */
  call_id: string;
  name: string;
  arguments: Record<string, unknown>;
  status: string;
  output_head?: string;
  output_tail?: string;
  output_line_count?: number;
  exit_code?: number;
  duration_ms: number;
};



export type ThreadPage = {
  items: TurnDetail[];
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
export async function getFleetTurn(
  workspaceId: string,
  fleetId: string,
  turnId: string,
  token: string,
): Promise<TurnDetail> {
  return request<TurnDetail>(
    `${fleetScope(workspaceId, fleetId)}/turns/${encodeURIComponent(turnId)}`,
    { method: "GET" },
    token,
  );
}

export async function listWorkspaceTurns(
  workspaceId: string,
  token: string,
  opts?: TurnsQuery,
): Promise<TurnsPage> {
  return TurnsPageSchema.parse(await request<unknown>(
    `/v1/workspaces/${encodeURIComponent(workspaceId)}/turns${buildQuery(opts)}`,
    { method: "GET" },
    token,
  ));
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
  budget_used_nanos?: string;
};

export type ActivityLiveFrame = (
  | {kind: typeof FRAME_KIND.TASK_CHANGED; task_id: string; resource_version: number}
  // A person's message, accepted and waiting for a runner: the id the send's
  // 202 names, who sent it, and what they typed. Fields past the identifier
  // are optional on the TYPE because the wire is untrusted.
  | {
      kind: typeof FRAME_KIND.TURN_ADMITTED;
      turn_id: string;
      actor: string;
      event_type?: EventTypeValue;
      message?: string;
      created_at?: string;
    }
  // The row as the run verb opened it: who raised it, how it entered, and
  // the row's own instant. Every field past the identifier is optional on the
  // TYPE because the wire is untrusted; the reducer guards each and falls back.
  // The counters ride here because a receive is when `events_processed` moves.
  | ({
      kind: typeof FRAME_KIND.TURN_RECEIVED;
      turn_id: string;
      actor: string;
      event_type?: EventTypeValue;
      created_at?: string;
      /** What a person typed, on a steer's frame only. */
      message?: string;
    } & FleetCountersSnapshot)

  | {
      kind: typeof FRAME_KIND.TOOL_CALL_STARTED;
      turn_id: string;
      name: string;
      args_redacted: unknown;
      /** Shared by every frame of one call. */
      call_id: string;
    }

  | {
      kind: typeof FRAME_KIND.TOOL_CALL_PROGRESS;
      turn_id: string;
      name: string;
      elapsed_ms: number;
      call_id: string;
    }

  | { kind: typeof FRAME_KIND.RUN_RESTARTED; turn_id: string; restarted_at?: string }
  | { kind: typeof FRAME_KIND.CHUNK; turn_id: string; text: string; text_kind: StreamTextKind; stream_start?: boolean; stream_contiguous?: boolean; stream_seq?: number }
  | {kind: typeof FRAME_KIND.TASK_DECISION_STARTED | typeof FRAME_KIND.TASK_DECISION_COMPLETED | typeof FRAME_KIND.TASK_TRANSFER_PROGRESS; turn_id: string; progress: unknown}
  | ({ kind: typeof FRAME_KIND.CONTEXT_PROGRESS; turn_id: string } & ContextProgress)

  // Status is required; output edges and exit codes depend on the tool.
  | {
      kind: typeof FRAME_KIND.TOOL_CALL_COMPLETED;
      turn_id: string;
      name: string;
      ms: number;
      call_id: string;
      status: string;
      output_head?: string;
      output_tail?: string;
      output_line_count?: number;
      exit_code?: number;
    }

  // The terminal row as the events list serves it, less the two scope columns
  // the channel names, plus the two fleet facts a run can change — a watcher
  // folds this in and reads nothing. Typed partial past the identifier for
  // the reason the opening bracket is.
  // The counters sit on this fleet-facts block, never on `TurnRow`: the row
  // carries one event's figures and the snapshot carries the fleet's totals.
  | ({ kind: typeof FRAME_KIND.TURN_COMPLETE; turn_id: string } & Partial<
      Omit<TurnRow, "turn_id" | "fleet_id" | "workspace_id">
    > & {
        fleet_status?: string;
        pending_approvals?: number;
        final_reply?: string;
      } & FleetCountersSnapshot)
  // A human has been asked about one of the fleet's actions; the count is how
  // many answers are owed, this one included.
  | ({
      kind: typeof FRAME_KIND.GATE_OPENED;
      gate_id: string;
      turn_id: string;
      pending_approvals: number;
    } & FleetCountersSnapshot)

  // A human answered, or the window closed with no answer. The event is null
  // for a gate raised outside a run (a standing grant).
  | ({
      kind: typeof FRAME_KIND.GATE_RESOLVED;
      gate_id: string;
      turn_id: string | null;
      status: string;
      resolved_by: string;
      pending_approvals: number;
    } & FleetCountersSnapshot)
) & { fencing_token?: string };

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

export type StreamTextKind = "answer" | "reasoning";

export type LiveFrame = ActivityLiveFrame | WorkspaceControlFrame;

// One multiplexed SSE frame from the workspace stream: a `LiveFrame` plus the
// `fleet_id` the backend spliced in, so the wall demultiplexes each frame to
// its tile. The backend guarantees the tag on every frame; a frame missing it
// is malformed and dropped by the client (never routed to a wrong tile).
export type WorkspaceLiveFrame = ActivityLiveFrame & { fleet_id: string };

export type WorkspaceFrame = WorkspaceLiveFrame | WorkspaceControlFrame;
