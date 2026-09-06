import type { EventDetail } from "@/lib/api/events";
import { OUTCOME } from "@/lib/events/event-summary";
import type { FleetEvent } from "@/lib/streaming/fleet-stream-row";

// The two rows the frame-reducer suites build on: a durable row as the events
// list serves it, and a rendered turn as the timeline holds it. Shared so the
// three suites that split by concern agree on one fixture.

export const MS_PER_SECOND = 1000 as const;

export function row(over: Partial<EventDetail> = {}): EventDetail {
  return {
    event_id: "e1",
    fleet_id: "z1",
    workspace_id: "ws1",
    actor: "fleet",
    event_type: "fleet_run",
    status: "processed",
    request_json: "{}",
    response_text: "hello",
    tokens: null,
    wall_ms: null,
    failure_label: null,
    checkpoint_id: null,
    resumes_event_id: null,
    created_at: MS_PER_SECOND,
    updated_at: MS_PER_SECOND,
    ...over,
  } as EventDetail;
}

export function evt(over: Partial<FleetEvent> = {}): FleetEvent {
  return {
    id: "e0",
    role: "assistant",
    actor: "fleet",
    text: "x",
    reply: "",
    outcome: OUTCOME.WORKING,
    failureLabel: null,
    failureDetail: null,
    createdAt: new Date(2000),
    status: "received",
    ...over,
  };
}
