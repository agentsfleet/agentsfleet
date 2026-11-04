import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";

const getFleetEventActionMock = vi.hoisted(() => vi.fn());
vi.mock("@/app/(dashboard)/w/[workspaceId]/fleets/actions", () => ({
  getFleetEventAction: getFleetEventActionMock,
}));

import { useEventDetail } from "@/components/domain/use-event-detail";
import type { EventDetail, EventLocator, EventRow } from "@/lib/api/events";

/** A list row as the events page hands it over: every column the list read
 *  carries, and neither of the two bodies it deliberately omits. */
function row(overrides: Partial<EventRow> = {}): EventRow {
  return {
    event_id: "ev_1",
    fleet_id: "zom_1",
    workspace_id: "ws_1",
    actor: "agent",
    event_type: "receive",
    status: "succeeded",
    tokens: 12,
    wall_ms: 340,
    failure_label: null,
    failure_detail: null,
    cost_nanos: null,
    resumes_event_id: null,
    created_at: "2026-04-30T00:00:00Z",
    updated_at: "2026-04-30T00:00:00.1Z",
    ...overrides,
  };
}

const detailFor = (r: EventRow): EventDetail =>
  ({ ...r, request_json: "{\"prompt\":\"hi\"}", response_text: "done", tool_calls: null });

beforeEach(() => vi.resetAllMocks());
afterEach(() => cleanup());

describe("useEventDetail", () => {
  it("asks for nothing while no row is open — a closed dialog costs no request", () => {
    const { result } = renderHook(() => useEventDetail(null));
    expect(result.current).toEqual({kind: "idle"});
    expect(getFleetEventActionMock).not.toHaveBeenCalled();
  });

  it("serves the bodies for the row that was opened", async () => {
    const r = row();
    getFleetEventActionMock.mockResolvedValue({ ok: true, data: detailFor(r) });

    const { result, rerender } = renderHook(({target}: {target: EventLocator | null}) => useEventDetail(target), {initialProps: {target: r as EventLocator | null}});

    await waitFor(() => expect(result.current).toEqual({kind: "ready", detail: detailFor(r)}));
    expect(getFleetEventActionMock).toHaveBeenCalledWith("ws_1", "zom_1", "ev_1");
    rerender({target: {...r}});
    expect(getFleetEventActionMock).toHaveBeenCalledOnce();
    rerender({target: null});
    expect(result.current).toEqual({kind: "idle"});
  });

  // The degraded path is deliberate: every other field in the dialog arrived
  // with the row and is already on screen, so a refused body blanks one panel
  // rather than the view.
  it("distinguishes a refused read from a record with no result", async () => {
    getFleetEventActionMock.mockResolvedValue({ ok: false, error: "not found" });

    const { result } = renderHook(() => useEventDetail(row()));

    await waitFor(() => expect(result.current).toEqual({kind: "failed"}));
    expect(getFleetEventActionMock).toHaveBeenCalledWith("ws_1", "zom_1", "ev_1");
  });

  // A Server Action that never resolves its envelope — transport gone, not a
  // refusal it could describe. Without the hook's own catch this rejection
  // escapes as an unhandled promise and the dialog keeps a stale body.
  it("distinguishes a rejected action without an unhandled promise", async () => {
    getFleetEventActionMock.mockRejectedValue(new Error("transport gone"));

    const { result } = renderHook(() => useEventDetail(row()));

    await waitFor(() => expect(result.current).toEqual({kind: "failed"}));
    expect(getFleetEventActionMock).toHaveBeenCalledOnce();
  });

  it("a body that lands after its row closed is dropped, never shown against another row", async () => {
    const slow = Promise.withResolvers<{ ok: true; data: EventDetail }>();
    getFleetEventActionMock.mockReturnValueOnce(slow.promise);

    const first = row();
    const { result, rerender } = renderHook(({ r }: { r: EventRow | null }) => useEventDetail(r), {
      initialProps: { r: first as EventRow | null },
    });

    // Close the dialog before the first read answers, then let it answer.
    rerender({ r: null });
    await act(async () => {slow.resolve({ ok: true, data: detailFor(first) }); await slow.promise;});

    expect(result.current).toEqual({kind: "idle"});
    expect(getFleetEventActionMock).toHaveBeenCalledOnce();
  });

  it("keeps an earlier workspace's delayed answer out of the selected native record", async () => {
    const first = Promise.withResolvers<{ok: true; data: EventDetail}>();
    const second = Promise.withResolvers<{ok: true; data: EventDetail}>();
    getFleetEventActionMock.mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);
    const old = row();
    const current = row({workspace_id: "ws_other", fleet_id: "fleet_other", event_id: "ev_other"});
    const {result, rerender} = renderHook(({target}) => useEventDetail(target), {initialProps: {target: old}});
    expect(result.current).toEqual({kind: "loading"});
    rerender({target: current});
    await act(async () => {second.resolve({ok: true, data: detailFor(current)}); await second.promise;});
    expect(result.current).toEqual({kind: "ready", detail: detailFor(current)});
    await act(async () => {first.resolve({ok: true, data: detailFor(old)}); await first.promise;});
    expect(result.current).toEqual({kind: "ready", detail: detailFor(current)});
    expect(getFleetEventActionMock.mock.calls).toEqual([["ws_1", "zom_1", "ev_1"], ["ws_other", "fleet_other", "ev_other"]]);
  });
});
