import type { EventDetail, EventRow } from "@/lib/api/events";
import { afterEach, beforeEach, vi } from "vitest";
import { __resetRegistryForTests } from "@/lib/streaming/fleet-stream-registry";
import { FakeEventSource } from "./fake-event-source";

export function row(over: Partial<EventDetail> = {}): EventDetail {
  const now = Date.UTC(2026, 4, 15, 18, 30, 0);
  return {
    event_id: "evt_seed",
    fleet_id: "zomb_a",
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
export const Z_A = "zomb_a";
export const Z_B = "zomb_b";
export const NO_SEED: EventRow[] = [];
export const IDLE_RELEASE_MS = 30_000;

export function setupRegistryTests(): void {
  beforeEach(() => {
    vi.useFakeTimers();
    FakeEventSource.install();
    __resetRegistryForTests();
  });

  afterEach(() => {
    __resetRegistryForTests();
    vi.useRealTimers();
    FakeEventSource.uninstall();
  });
}

export function sourceAt(index: number): FakeEventSource {
  const source = FakeEventSource.instances.at(index);
  if (!source) throw new Error(`No EventSource at index ${index}`);
  return source;
}
