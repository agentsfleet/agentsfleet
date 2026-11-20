import type { TurnDetail } from "@/lib/api/turns";
import { OUTCOME } from "@/lib/turns/turn-summary";
import type { FleetTurn } from "@/lib/streaming/fleet-stream-row";

// The two rows the frame-reducer suites build on: a durable row as the turns
// list serves it, and a rendered turn as the timeline holds it. Shared so the
// three suites that split by concern agree on one fixture.

export const MS_PER_SECOND = 1000 as const;

export function row(over: Partial<TurnDetail> = {}): TurnDetail {
  return {
    turn_id: "e1",
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
    resumes_turn_id: null,
    created_at: "1970-01-01T00:00:01Z",
    updated_at: "1970-01-01T00:00:01Z",
    ...over,
  } as TurnDetail;
}

export function evt(over: Partial<FleetTurn> = {}): FleetTurn {
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
