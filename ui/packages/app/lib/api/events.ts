import { request } from "./client";

// Operator-visible event rows from `core.zombie_events`. Mirrors the
// server's `EventRow` envelope verbatim (no shim, no rename) — the
// dashboard renders the same shape it queries.

export type EventStatus = "received" | "processed" | "agent_error" | "gate_blocked";
export type EventType = "chat" | "webhook" | "cron" | "continuation";

export type EventRow = {
  event_id: string;
  zombie_id: string;
  workspace_id: string;
  actor: string;
  event_type: EventType | string;
  status: EventStatus | string;
  request_json: string;
  response_text: string | null;
  tokens: number | null;
  wall_ms: number | null;
  failure_label: string | null;
  checkpoint_id: string | null;
  resumes_event_id: string | null;
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
  since?: string;
  zombie_id?: string;
  limit?: number;
};

function buildQuery(opts?: EventsQuery): string {
  if (!opts) return "";
  const params = new URLSearchParams();
  if (opts.cursor) params.set("cursor", opts.cursor);
  if (opts.actor) params.set("actor", opts.actor);
  if (opts.since) params.set("since", opts.since);
  if (opts.zombie_id) params.set("zombie_id", opts.zombie_id);
  if (opts.limit != null) params.set("limit", String(opts.limit));
  const qs = params.toString();
  return qs.length > 0 ? `?${qs}` : "";
}

export async function listZombieEvents(
  workspaceId: string,
  zombieId: string,
  token: string,
  opts?: Omit<EventsQuery, "zombie_id">,
): Promise<EventsPage> {
  return request<EventsPage>(
    `/v1/workspaces/${workspaceId}/zombies/${zombieId}/events${buildQuery(opts)}`,
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
    `/v1/workspaces/${workspaceId}/events${buildQuery(opts)}`,
    { method: "GET" },
    token,
  );
}

// Live progress frames published on `zombie:{id}:activity` (Redis pub/sub),
// fanned out to subscribers as SSE messages by the backend handler. The
// backend authoritatively shapes these — keep `FRAME_KIND` in sync with
// the KIND_* constants in src/zombie/activity_publisher.zig.
export const FRAME_KIND = {
  EVENT_RECEIVED: "event_received",
  TOOL_CALL_STARTED: "tool_call_started",
  TOOL_CALL_PROGRESS: "tool_call_progress",
  CHUNK: "chunk",
  TOOL_CALL_COMPLETED: "tool_call_completed",
  EVENT_COMPLETE: "event_complete",
} as const;

export type FrameKind = (typeof FRAME_KIND)[keyof typeof FRAME_KIND];

export type LiveFrame =
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
  | { kind: typeof FRAME_KIND.EVENT_COMPLETE; event_id: string; status: string };

// Same-origin URL for the SSE stream. The path is intercepted by the
// Next Route Handler at app/backend/.../events/stream/route.ts which
// injects the api-audience Bearer token server-side.
export function streamZombieEventsUrl(workspaceId: string, zombieId: string): string {
  return (
    `/backend/v1/workspaces/${encodeURIComponent(workspaceId)}` +
    `/zombies/${encodeURIComponent(zombieId)}/events/stream`
  );
}
