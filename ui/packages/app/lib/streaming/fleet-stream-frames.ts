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
  const fromBackfill = rows.filter((r) => !seen.has(r.event_id)).map(rowToEvent);
  return [...fromBackfill, ...prev].sort(
    (a, b) => a.createdAt.getTime() - b.createdAt.getTime(),
  );
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
