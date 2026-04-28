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
