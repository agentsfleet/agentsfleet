import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { TurnDetail } from "@/lib/api/events";
import { rowToEvent, type FleetEvent } from "@/lib/streaming/fleet-stream-row";
import { stoppableTurn } from "./stoppable-turn";

const { stopTurn, readTurnDetailRoute } = vi.hoisted(() => ({ stopTurn: vi.fn(), readTurnDetailRoute: vi.fn() }));
vi.mock("@/lib/api/fleet-turn-stop", async (actual) => ({ ...await actual<object>(), stopTurn }));
vi.mock("@/lib/streaming/fleet-stream-detail-reader", () => ({ readTurnDetailRoute }));

import { useTurnStop } from "./useTurnStop";
import { TURN_STOP_KIND } from "@/lib/api/fleet-turn-stop";

const WORKSPACE = "0199a801-1000-7000-8000-0000000000a1";
const FLEET = "0199a801-1000-7000-8000-0000000000b1";
const FIRST = "0199a801-1000-7000-8000-0000000000e1";
const SECOND = "0199a801-1000-7000-8000-0000000000e2";

function row(turn_id: string, status: string, over: Partial<TurnDetail> = {}): TurnDetail {
  return { fleet_id: FLEET, turn_id, workspace_id: WORKSPACE, actor: "steer:user_1", event_type: "chat", status,
    request_json: JSON.stringify({ message: "deploy" }), response_text: null, tool_calls: null, tokens: null,
    wall_ms: null, cost_nanos: null, failure_label: null, failure_detail: null, resumes_turn_id: null,
    created_at: "2026-11-20T10:00:00.000Z", updated_at: "2026-11-20T10:00:00.000Z", ...over };
}

function event(turnId: string, status: string, over: Partial<TurnDetail> = {}): FleetEvent {
  return rowToEvent(row(turnId, status, over));
}

afterEach(() => {
  stopTurn.mockReset();
  readTurnDetailRoute.mockReset();
});

describe("stoppableTurn", () => {
  it("names no turn once every turn settled, cancelled included", () => {
    expect(stoppableTurn([event(FIRST, "processed"), event(SECOND, "cancelled"), event(FIRST, "fleet_error")])).toBeNull();
  });

  it("prefers the running turn over a queued one behind it", () => {
    expect(stoppableTurn([event(FIRST, "queued"), event(SECOND, "received")])).toBe(SECOND);
  });

  it("falls back to the oldest waiting turn: queued, or parked on an approval", () => {
    expect(stoppableTurn([event(FIRST, "processed"), event(SECOND, "queued")])).toBe(SECOND);
    expect(stoppableTurn([event(FIRST, "gate_blocked")])).toBe(FIRST);
  });

  it("skips a turn refused at a gate and a send the daemon has not named", () => {
    expect(stoppableTurn([event(FIRST, "gate_blocked", { failure_label: "approval_denied" })])).toBeNull();
    expect(stoppableTurn([{ ...event(FIRST, "queued"), clientTimestamp: true }])).toBeNull();
    expect(stoppableTurn([{ ...event(FIRST, "received"), status: "optimistic" }])).toBeNull();
  });
});

describe("useTurnStop", () => {
  it("applies the cancelled row the daemon answered with", async () => {
    const cancelled = row(FIRST, "cancelled");
    stopTurn.mockResolvedValueOnce({ kind: TURN_STOP_KIND.STOPPED, turn: cancelled });
    const reconcile = vi.fn();
    const { result } = renderHook(() => useTurnStop(WORKSPACE, FLEET, [event(FIRST, "received")], reconcile));
    expect(result.current.visible).toBe(true);
    act(() => result.current.stop());
    expect(result.current.stopping).toBe(true);
    await waitFor(() => expect(reconcile).toHaveBeenCalledWith([cancelled]));
    expect(stopTurn).toHaveBeenCalledWith(WORKSPACE, FLEET, FIRST, expect.any(AbortSignal));
    await waitFor(() => expect(result.current.stopping).toBe(false));
  });

  it("reads the saved answer again when the turn finished first", async () => {
    const answered = row(FIRST, "processed", { response_text: "Done." });
    stopTurn.mockResolvedValueOnce({ kind: TURN_STOP_KIND.FINISHED });
    readTurnDetailRoute.mockResolvedValueOnce({ ok: true, data: answered });
    const reconcile = vi.fn();
    const { result } = renderHook(() => useTurnStop(WORKSPACE, FLEET, [event(FIRST, "received")], reconcile));
    act(() => result.current.stop());
    await waitFor(() => expect(reconcile).toHaveBeenCalledWith([answered]));
  });

  it("is hidden and does nothing while no turn runs", () => {
    const reconcile = vi.fn();
    const { result } = renderHook(() => useTurnStop(WORKSPACE, FLEET, [event(FIRST, "processed")], reconcile));
    expect(result.current.visible).toBe(false);
    act(() => result.current.stop());
    expect(stopTurn).not.toHaveBeenCalled();
  });
});
