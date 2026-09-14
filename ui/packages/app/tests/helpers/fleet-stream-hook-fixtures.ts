import { renderHook } from "@testing-library/react";
import { useFleetEventStream } from "@/components/domain/useFleetEventStream";
import type { EventDetail, EventRow } from "@/lib/api/events";

export function row(over: Partial<EventDetail> = {}): EventDetail {
  const now = Date.UTC(2026, 4, 15, 18, 30, 0);
  return {
    event_id: "evt_seed",
    fleet_id: "zomb_1",
    workspace_id: "ws_1",
    actor: "alice@example.com",
    event_type: "chat",
    status: "processed",
    request_json: "{}",
    response_text: "seed body",
    tokens: 1,
    wall_ms: 10,
    cost_nanos: null,
    failure_label: null,
    failure_detail: null,
    checkpoint_id: null,
    resumes_event_id: null,
    created_at: now,
    updated_at: now,
    ...over,
  };
}

export const WS = "ws_1";
export const ZID = "zomb_1";

export function mount(initial: EventRow[] = []) {
  return renderHook(() => useFleetEventStream(WS, ZID, initial));
}
