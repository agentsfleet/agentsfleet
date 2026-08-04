import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, renderHook, waitFor } from "@testing-library/react";

const getFleetEventActionMock = vi.hoisted(() => vi.fn());
vi.mock("@/app/(dashboard)/w/[workspaceId]/fleets/actions", () => ({
  getFleetEventAction: getFleetEventActionMock,
}));

import { useEventDetail } from "@/components/domain/use-event-detail";
import type { EventDetail, EventRow } from "@/lib/api/events";

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
    checkpoint_id: null,
    cost_nanos: null,
    created_at: 1_777_507_200_000,
    updated_at: 1_777_507_200_100,
    ...overrides,
  } as EventRow;
}

const detailFor = (r: EventRow): EventDetail =>
  ({ ...r, request_json: "{\"prompt\":\"hi\"}", response_text: "done" }) as EventDetail;

/** A promise plus the handle to settle it later — needed to observe the state
 *  between "request issued" and "request answered". */
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (cause: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

beforeEach(() => vi.clearAllMocks());
afterEach(() => cleanup());

describe("useEventDetail", () => {
  it("asks for nothing while no row is open — a closed dialog costs no request", () => {
    const { result } = renderHook(() => useEventDetail(null));
    expect(result.current).toBeNull();
    expect(getFleetEventActionMock).not.toHaveBeenCalled();
  });

  it("serves the bodies for the row that was opened", async () => {
    const r = row();
    getFleetEventActionMock.mockResolvedValue({ ok: true, data: detailFor(r) });

    const { result } = renderHook(() => useEventDetail(r));

    await waitFor(() => expect(result.current?.response_text).toBe("done"));
    expect(getFleetEventActionMock).toHaveBeenCalledWith("ws_1", "zom_1", "ev_1");
  });

  // The degraded path is deliberate: every other field in the dialog arrived
  // with the row and is already on screen, so a refused body blanks one panel
  // rather than the view.
  it("a refused read renders as no detail, not as an error", async () => {
    getFleetEventActionMock.mockResolvedValue({ ok: false, error: "not found" });

    const { result } = renderHook(() => useEventDetail(row()));

    await waitFor(() => expect(getFleetEventActionMock).toHaveBeenCalled());
    expect(result.current).toBeNull();
  });

  // A Server Action that never resolves its envelope — transport gone, not a
  // refusal it could describe. Without the hook's own catch this rejection
  // escapes as an unhandled promise and the dialog keeps a stale body.
  it("a rejected action degrades to no detail instead of escaping", async () => {
    getFleetEventActionMock.mockRejectedValue(new Error("transport gone"));

    const { result } = renderHook(() => useEventDetail(row()));

    await waitFor(() => expect(getFleetEventActionMock).toHaveBeenCalled());
    expect(result.current).toBeNull();
  });

  it("a body that lands after its row closed is dropped, never shown against another row", async () => {
    const slow = deferred<{ ok: true; data: EventDetail }>();
    getFleetEventActionMock.mockReturnValueOnce(slow.promise);

    const first = row();
    const { result, rerender } = renderHook(({ r }: { r: EventRow | null }) => useEventDetail(r), {
      initialProps: { r: first as EventRow | null },
    });

    // Close the dialog before the first read answers, then let it answer.
    rerender({ r: null });
    slow.resolve({ ok: true, data: detailFor(first) });
    await slow.promise;

    expect(result.current).toBeNull();
  });
});
