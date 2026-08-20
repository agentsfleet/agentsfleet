import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, renderHook } from "@testing-library/react";

const refreshMock = vi.hoisted(() => vi.fn());
vi.mock("next/navigation", () => ({
  useRouter: () => ({ refresh: refreshMock }),
}));

import {
  REFRESH_DEBOUNCE_MS,
  useRefreshSummariesOnCompletion,
} from "@/components/domain/useRefreshOnCompletion";
import type { FleetEvent } from "@/lib/streaming/fleet-stream-frames";

function terminal(id: string): FleetEvent {
  return { id, status: "processed" } as FleetEvent;
}

function running(id: string): FleetEvent {
  return { id, status: "received" } as FleetEvent;
}

beforeEach(() => {
  vi.useFakeTimers();
  refreshMock.mockReset();
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe("useRefreshSummariesOnCompletion", () => {
  it("test_terminal_refresh_debounced: a burst of completions coalesces into one refresh", () => {
    const { rerender } = renderHook(
      ({ events }: { events: FleetEvent[] }) =>
        useRefreshSummariesOnCompletion([], events),
      { initialProps: { events: [running("e1")] } },
    );

    // Five completions land inside the window — the retired shape refreshed
    // the whole page graph five times.
    rerender({ events: [terminal("e1")] });
    rerender({ events: [terminal("e1"), terminal("e2")] });
    rerender({ events: [terminal("e1"), terminal("e2"), terminal("e3")] });
    rerender({
      events: [terminal("e1"), terminal("e2"), terminal("e3"), terminal("e4")],
    });
    rerender({
      events: [
        terminal("e1"),
        terminal("e2"),
        terminal("e3"),
        terminal("e4"),
        terminal("e5"),
      ],
    });
    expect(refreshMock).not.toHaveBeenCalled();

    vi.advanceTimersByTime(REFRESH_DEBOUNCE_MS);
    expect(refreshMock).toHaveBeenCalledTimes(1);
  });

  it("the trailing completion is never lost", () => {
    const { rerender } = renderHook(
      ({ events }: { events: FleetEvent[] }) =>
        useRefreshSummariesOnCompletion([], events),
      { initialProps: { events: [running("e1")] } },
    );

    rerender({ events: [terminal("e1")] });
    vi.advanceTimersByTime(REFRESH_DEBOUNCE_MS);
    expect(refreshMock).toHaveBeenCalledTimes(1);

    // A later, separate completion earns its own refresh.
    rerender({ events: [terminal("e1"), terminal("e2")] });
    vi.advanceTimersByTime(REFRESH_DEBOUNCE_MS);
    expect(refreshMock).toHaveBeenCalledTimes(2);
  });

  it("unmount cancels a pending refresh", () => {
    const { rerender, unmount } = renderHook(
      ({ events }: { events: FleetEvent[] }) =>
        useRefreshSummariesOnCompletion([], events),
      { initialProps: { events: [running("e1")] } },
    );

    rerender({ events: [terminal("e1")] });
    unmount();
    vi.advanceTimersByTime(REFRESH_DEBOUNCE_MS * 2);
    expect(refreshMock).not.toHaveBeenCalled();
  });

  it("already-terminal seed rows never trigger a refresh", () => {
    const seeded = [terminal("e1")];
    renderHook(() => useRefreshSummariesOnCompletion([], seeded));
    vi.advanceTimersByTime(REFRESH_DEBOUNCE_MS * 2);
    expect(refreshMock).not.toHaveBeenCalled();
  });
});
