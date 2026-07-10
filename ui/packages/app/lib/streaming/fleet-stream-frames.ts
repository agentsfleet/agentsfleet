import { FRAME_KIND, type EventRow, type LiveFrame } from "@/lib/api/events";

// Pure frame-transform helpers shared by the streaming registry.
// Nothing here touches Map state, EventSource, or React. Splitting
// these out keeps the registry's lifecycle file under the LENGTH GATE
// and the helpers unit-testable without spinning up a subscription.

export const AGENTSFLEET_EVENT_STATUS = {
  RECEIVED: "received",
  PROCESSED: "processed",
  AGENT_ERROR: "fleet_error",
  GATE_BLOCKED: "gate_blocked",
  OPTIMISTIC: "optimistic",
  FAILED: "failed",
} as const;

export type FleetEventStatus =
  (typeof AGENTSFLEET_EVENT_STATUS)[keyof typeof AGENTSFLEET_EVENT_STATUS];

export type FleetEvent = {
  id: string;
  role: "user" | "assistant" | "system";
  actor: string;
  text: string;
  createdAt: Date;
  status: FleetEventStatus;
  custom?: { requestJson?: string | null; reason?: string };
};

export function mergeBackfill(
  prev: FleetEvent[],
  rows: EventRow[],
): FleetEvent[] {
  const seen = new Set(prev.map((e) => e.id));
  // A terminal backfill row is authoritative over a live row with the same
  // id — an event that straddled an outage may sit here as a partial chunk
  // accumulation, and the durable row carries the full final text + status.
  // An in-progress ("received") backfill row never clobbers live chunks:
  // the live accumulation is newer than the list snapshot.
  const authoritative = new Map<string, EventRow>();
  for (const r of rows) {
    if (seen.has(r.event_id) && r.status !== AGENTSFLEET_EVENT_STATUS.RECEIVED) {
      authoritative.set(r.event_id, r);
    }
  }
  const kept = prev.map((e) => {
    const replacement = authoritative.get(e.id);
    return replacement ? rowToEvent(replacement) : e;
  });
  const fromBackfill = rows.filter((r) => !seen.has(r.event_id)).map(rowToEvent);
  return [...fromBackfill, ...kept].sort(
    (a, b) => a.createdAt.getTime() - b.createdAt.getTime(),
  );
}

// The newest server-confirmed `created_at` across the rows, folded into the
// running watermark. Live SSE frames are stamped with the CLIENT clock and
// must never advance this — a skewed client would push the backfill's lower
// bound into the server's future and silently recover nothing.
export function maxServerCreatedAt(
  current: number | null,
  rows: EventRow[],
): number | null {
  let max = current;
  for (const r of rows) {
    if (typeof r.created_at === "number" && (max === null || r.created_at > max)) {
      max = r.created_at;
    }
  }
  return max;
}

// Epoch ms → the 20-char `YYYY-MM-DDTHH:MM:SSZ` shape the upstream `since`
// parser accepts (no fractional seconds).
export function rfc3339Seconds(ms: number): string {
  return `${new Date(Math.max(ms, 0)).toISOString().slice(0, 19)}Z`;
}

export function applyLiveFrame(
  prev: FleetEvent[],
  frame: LiveFrame,
): FleetEvent[] {
  switch (frame.kind) {
    case FRAME_KIND.EVENT_RECEIVED:
      return applyEventReceived(prev, frame);
    case FRAME_KIND.CHUNK:
      return applyChunk(prev, frame);
    case FRAME_KIND.EVENT_COMPLETE:
      return applyEventComplete(prev, frame);
    default:
      return prev;
  }
}

export function actorToRole(actor: string): "user" | "assistant" | "system" {
  if (actor.startsWith("steer:")) return "user";
  if (actor === "fleet") return "assistant";
  return "system";
}

// ── internals ────────────────────────────────────────────────────────────

function rowToEvent(row: EventRow): FleetEvent {
  return {
    id: row.event_id,
    role: actorToRole(row.actor),
    actor: row.actor,
    text: row.response_text ?? "",
    createdAt: new Date(row.created_at),
    status: row.status as FleetEventStatus,
    custom: { requestJson: row.request_json },
  };
}

function applyEventReceived(
  prev: FleetEvent[],
  frame: Extract<LiveFrame, { kind: typeof FRAME_KIND.EVENT_RECEIVED }>,
): FleetEvent[] {
  if (prev.some((e) => e.id === frame.event_id)) return prev;
  return [
    ...prev,
    {
      id: frame.event_id,
      role: actorToRole(frame.actor),
      actor: frame.actor,
      text: "",
      createdAt: new Date(),
      status: "received",
    },
  ];
}

function applyChunk(
  prev: FleetEvent[],
  frame: Extract<LiveFrame, { kind: typeof FRAME_KIND.CHUNK }>,
): FleetEvent[] {
  const existing = prev.find((e) => e.id === frame.event_id);
  if (!existing) {
    return [
      ...prev,
      {
        id: frame.event_id,
        role: "assistant",
        actor: "fleet",
        text: frame.text,
        createdAt: new Date(),
        status: "received",
      },
    ];
  }
  return prev.map((e) =>
    e === existing
      ? {
          ...e,
          role: e.role === "user" ? "user" : "assistant",
          text: e.text + frame.text,
        }
      : e,
  );
}

function applyEventComplete(
  prev: FleetEvent[],
  frame: Extract<LiveFrame, { kind: typeof FRAME_KIND.EVENT_COMPLETE }>,
): FleetEvent[] {
  const existing = prev.find((e) => e.id === frame.event_id);
  if (!existing) return prev;
  return prev.map((e) =>
    e === existing
      ? { ...e, status: (frame.status ?? AGENTSFLEET_EVENT_STATUS.PROCESSED) as FleetEventStatus }
      : e,
  );
}
