import { renderHook, act } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { useResettableTimeout } from "./use-resettable-timeout";

describe("useResettableTimeout", () => {
  it("fires the callback after the requested delay", () => {
    vi.useFakeTimers();
    const cb = vi.fn();
    const { result } = renderHook(() => useResettableTimeout());
    act(() => {
      result.current.start(cb, 1000);
    });
    expect(cb).not.toHaveBeenCalled();
    act(() => {
      vi.advanceTimersByTime(1000);
    });
    expect(cb).toHaveBeenCalledTimes(1);
  });

  it("cancels a pending callback when start is called again with a fresh closure", () => {
    vi.useFakeTimers();
    const first = vi.fn();
    const second = vi.fn();
    const { result } = renderHook(() => useResettableTimeout());
    act(() => {
      result.current.start(first, 1000);
    });
    act(() => {
      vi.advanceTimersByTime(500);
    });
    act(() => {
      result.current.start(second, 1000);
    });
    act(() => {
      vi.advanceTimersByTime(500);
    });
    // Original window has elapsed; the first callback must not have fired.
    expect(first).not.toHaveBeenCalled();
    act(() => {
      vi.advanceTimersByTime(500);
    });
    expect(second).toHaveBeenCalledTimes(1);
    expect(first).not.toHaveBeenCalled();
  });

  it("cancel() prevents the pending callback from firing", () => {
    vi.useFakeTimers();
    const cb = vi.fn();
    const { result } = renderHook(() => useResettableTimeout());
    act(() => {
      result.current.start(cb, 1000);
      result.current.cancel();
    });
    act(() => {
      vi.advanceTimersByTime(2000);
    });
    expect(cb).not.toHaveBeenCalled();
  });

  it("clears the pending callback on unmount", () => {
    vi.useFakeTimers();
    const cb = vi.fn();
    const { result, unmount } = renderHook(() => useResettableTimeout());
    act(() => {
      result.current.start(cb, 1000);
    });
    unmount();
    act(() => {
      vi.advanceTimersByTime(2000);
    });
    expect(cb).not.toHaveBeenCalled();
  });

  it("returns a stable identity across re-renders (safe to read in event handlers)", () => {
    const { result, rerender } = renderHook(() => useResettableTimeout());
    const first = result.current;
    rerender();
    const second = result.current;
    expect(second).toBe(first);
    expect(second.start).toBe(first.start);
    expect(second.cancel).toBe(first.cancel);
  });
});
