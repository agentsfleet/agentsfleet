import { streamWrapper } from "./fleet-streams";
import { renderHook } from "@testing-library/react";
import { useFleetEventStream } from "@/components/domain/conversation/stream/useFleetEventStream";
import type { TurnDetail, TurnRow } from "@/lib/api/events";

export function row(over: Partial<TurnDetail> = {}): TurnDetail {
  const now = Date.UTC(2026, 4, 15, 18, 30, 0);
  return {
    turn_id: "evt_seed",
    fleet_id: "zomb_1",
    workspace_id: "ws_1",
    actor: "alice@example.com",
    event_type: "chat",
    status: "processed",
    request_json: "{}",
    response_text: "seed body",
    tool_calls: null,
    tokens: 1,
    wall_ms: 10,
    cost_nanos: null,
    failure_label: null,
    failure_detail: null,
    resumes_turn_id: null,
    created_at: new Date(now).toISOString(),
    updated_at: new Date(now).toISOString(),
    ...over,
  };
}

export const WS = "ws_1";
export const ZID = "zomb_1";

export function mount(initial: TurnRow[] = []) {
  return renderHook(() => useFleetEventStream(WS, ZID, initial), {wrapper: streamWrapper});
}
