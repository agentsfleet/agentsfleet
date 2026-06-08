import { renderHook, act } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { useResettableTimeout } from "./use-resettable-timeout";

describe("useResettableTimeout", () => {
  it("fires the callback after the requested delay", () => {
    vi.useFakeTimers();
    const cb = vi.fn();
    const { result } = renderHook(() => useResettableTimeout());
    act(() => {
      result.current.start(cb, MS_PER_SECOND);
    });
    expect(cb).not.toHaveBeenCalled();
    act(() => {
      vi.advanceTimersByTime(MS_PER_SECOND);
    });
    expect(cb).toHaveBeenCalledTimes(1);
  });

  it("cancels a pending callback when start is called again with a fresh closure", () => {
    vi.useFakeTimers();
    const first = vi.fn();
    const second = vi.fn();
    const { result } = renderHook(() => useResettableTimeout());
    act(() => {
      result.current.start(first, MS_PER_SECOND);
    });
    act(() => {
      vi.advanceTimersByTime(500);
    });
    act(() => {
      result.current.start(second, MS_PER_SECOND);
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
      result.current.start(cb, MS_PER_SECOND);
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
      result.current.start(cb, MS_PER_SECOND);
    });
    unmount();
    act(() => {
      vi.advanceTimersByTime(2000);
    });
    expect(cb).not.toHaveBeenCalled();
  });

  it("cancel() is a safe no-op when no timer is pending", () => {
    const { result } = renderHook(() => useResettableTimeout());
    // No start() was called — handle.current is null, so cancel() takes the
    // `if (handle.current !== null)` false branch and does nothing.
    expect(() => act(() => result.current.cancel())).not.toThrow();
    // A subsequent start still works after the no-op cancel.
    vi.useFakeTimers();
    const cb = vi.fn();
    act(() => result.current.start(cb, 100));
    act(() => vi.advanceTimersByTime(100));
    expect(cb).toHaveBeenCalledTimes(1);
    vi.useRealTimers();
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
const MS_PER_SECOND = 1000 as const;
