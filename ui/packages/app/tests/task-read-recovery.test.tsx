import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { useTaskRead } from "@/components/domain/chat/use-task-read";
import type { TaskRead } from "@/lib/api/tasks";
afterEach(() => {cleanup(); vi.useRealTimers();});

it("retries transient read failures on a bounded cadence and cancels recovery on unmount", async () => {
  vi.useFakeTimers();
  const read = vi.fn<(signal: AbortSignal) => Promise<TaskRead<string>>>()
    .mockResolvedValueOnce({kind: "failed", reason: "unreachable"})
    .mockResolvedValueOnce({kind: "ready", value: "confirmed"});
  const view = renderHook(() => useTaskRead(read, 0));
  await act(async () => {});
  expect(view.result.current.resource.kind).toBe("failed");
  await act(async () => vi.advanceTimersByTimeAsync(4999));
  expect(read).toHaveBeenCalledTimes(1);
  await act(async () => vi.advanceTimersByTimeAsync(1));
  expect(view.result.current.resource).toEqual({kind: "ready", value: "confirmed"});
  view.unmount();
  expect(read.mock.calls[1]?.[0].aborted).toBe(true);
  await act(async () => vi.advanceTimersByTimeAsync(10000));
  expect(read).toHaveBeenCalledTimes(2);
});

it.each(["access", "missing"] as const)("does not poll past %s denial", async reason => {
  vi.useFakeTimers();
  const read = vi.fn<(signal: AbortSignal) => Promise<TaskRead<string>>>().mockResolvedValue({kind: "failed", reason});
  renderHook(() => useTaskRead(read, 0));
  await act(async () => vi.advanceTimersByTimeAsync(20000));
  expect(read).toHaveBeenCalledTimes(1);
});
