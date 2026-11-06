import type { TaskRead } from "@/lib/api/tasks";
import { recoverTaskRead } from "@/components/domain/chat/task-read-recovery";
import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { useTaskRead } from "@/components/domain/chat/use-task-read";
afterEach(() => {cleanup(); vi.useRealTimers(); vi.restoreAllMocks();});

it("retries transient read failures on a bounded cadence and cancels recovery on unmount", async () => {
  vi.useFakeTimers();
  vi.spyOn(Math, "random").mockReturnValue(0);
  const read = vi.fn<(signal: AbortSignal) => Promise<TaskRead<string>>>()
    .mockResolvedValueOnce({kind: "failed", reason: "unreachable"})
    .mockResolvedValueOnce({kind: "ready", value: "confirmed"});
  const view = renderHook(() => useTaskRead(read, 0));
  await act(async () => {});
  expect(view.result.current.resource.kind).toBe("failed");
  await act(async () => vi.advanceTimersByTimeAsync(499));
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

it("stops after four attempts with increasing backoff and lets the person retry after exhaustion", async () => {
  vi.useFakeTimers(); vi.setSystemTime(0);
  vi.spyOn(Math, "random").mockReturnValue(0);
  const times: number[] = [];
  const read = vi.fn<(signal: AbortSignal) => Promise<TaskRead<string>>>().mockImplementation(async () => {
    times.push(Date.now()); return {kind: "failed", reason: "unreachable"};
  });
  const view = renderHook(() => useTaskRead(read, 0));
  await act(async () => vi.advanceTimersByTimeAsync(100000));
  expect(times).toEqual([0, 500, 1500, 3500]);
  expect(view.result.current.resource).toEqual({kind: "failed", reason: "unreachable", previous: null});
  read.mockResolvedValue({kind: "ready", value: "recovered"});
  await act(async () => view.result.current.retry());
  expect(read).toHaveBeenCalledTimes(5);
  expect(view.result.current.resource).toEqual({kind: "ready", value: "recovered"});
});

it.each([[0, 500], [0.5, 750], [0.999, 1000]])("spreads retries using random sample %s and a %sms first delay", async (random, delay) => {
  vi.useFakeTimers(); vi.spyOn(Math, "random").mockReturnValue(random);
  const read = vi.fn<(signal: AbortSignal) => Promise<TaskRead<string>>>()
    .mockResolvedValueOnce({kind: "failed", reason: "timeout"}).mockResolvedValueOnce({kind: "ready", value: "saved"});
  renderHook(() => useTaskRead(read, 0));
  await act(async () => vi.advanceTimersByTimeAsync(delay - 1));
  expect(read).toHaveBeenCalledTimes(1);
  await act(async () => vi.advanceTimersByTimeAsync(1));
  expect(read).toHaveBeenCalledTimes(2);
});

it("aborts an outstanding retry delay when its scoped owner unmounts", async () => {
  vi.useFakeTimers(); vi.spyOn(Math, "random").mockReturnValue(0);
  const read = vi.fn<(signal: AbortSignal) => Promise<TaskRead<string>>>().mockResolvedValue({kind: "failed", reason: "unreachable"});
  const view = renderHook(() => useTaskRead(read, 0));
  await act(async () => vi.advanceTimersByTimeAsync(0));
  view.unmount();
  await act(async () => vi.advanceTimersByTimeAsync(100000));
  expect(read).toHaveBeenCalledTimes(1);
  expect(read.mock.calls[0]?.[0].aborted).toBe(true);
});

it("does not schedule recovery after the observation revokes its owner", async () => {
  vi.useFakeTimers();
  const controller = new AbortController();
  const read = vi.fn<(signal: AbortSignal) => Promise<TaskRead<string>>>().mockResolvedValue({kind: "failed", reason: "unreachable"});
  const observed = vi.fn(() => controller.abort());
  await recoverTaskRead(read, controller.signal, observed);
  await vi.advanceTimersByTimeAsync(100000);
  expect(observed).toHaveBeenCalledTimes(1);
  expect(observed).toHaveBeenCalledWith({kind: "failed", reason: "unreachable"});
  expect(read).toHaveBeenCalledTimes(1);
  await recoverTaskRead(read, controller.signal, observed);
  expect(read).toHaveBeenCalledTimes(1);
});
