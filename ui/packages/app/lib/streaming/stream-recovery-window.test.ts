import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { StreamRecoveryWindow } from "./stream-recovery-window";

const STABLE_MS = 30_000;
const GRACE_MS = 25_000;
const SILENCE_MS = 45_000;

beforeEach(() => vi.useFakeTimers());
afterEach(() => {
  vi.restoreAllMocks();
  vi.useRealTimers();
});

function stableWindow() {
  const window = new StreamRecoveryWindow();
  window.opened(vi.fn());
  window.received(vi.fn());
  vi.advanceTimersByTime(STABLE_MS);
  window.received(vi.fn());
  return window;
}

it("should require actual arrivals spanning thirty seconds to establish stability", () => {
  const window = new StreamRecoveryWindow();
  expect(window.isStable()).toBe(false);
  expect(window.isStale()).toBe(false);
  window.opened(vi.fn());
  vi.advanceTimersByTime(STABLE_MS);
  expect(window.isStable()).toBe(false);
  window.received(vi.fn());
  expect(vi.getTimerCount()).toBe(1);
  vi.advanceTimersByTime(STABLE_MS - 1);
  window.received(vi.fn());
  expect(window.isStable()).toBe(false);
  vi.advanceTimersByTime(1);
  window.received(vi.fn());
  expect(window.isStable()).toBe(true);
  window.dispose();
  expect(window.isStable()).toBe(false);
});

it("should report a failure before open or immediately after open without grace", () => {
  const window = new StreamRecoveryWindow();
  const report = vi.fn();
  window.reportLoss(report);
  window.opened(vi.fn());
  window.reportLoss(report);
  expect(report).toHaveBeenCalledTimes(2);
  expect(vi.getTimerCount()).toBe(0);
});

it("should never extend a grace deadline on repeated failures or HTTP opens", () => {
  const window = stableWindow();
  const report = vi.fn();
  window.reportLoss(report);
  vi.advanceTimersByTime(GRACE_MS - 1);
  window.opened(vi.fn());
  window.reportLoss(report);
  expect(report).not.toHaveBeenCalled();
  expect(vi.getTimerCount()).toBe(1);
  vi.advanceTimersByTime(1);
  expect(report).toHaveBeenCalledTimes(1);
  window.reportLoss(report);
  expect(report).toHaveBeenCalledTimes(2);
  expect(vi.getTimerCount()).toBe(0);
});

it.each(["received", "dispose"] as const)("should cancel the warning on %s", (action) => {
  const window = stableWindow();
  const report = vi.fn();
  window.reportLoss(report);
  window[action](vi.fn());
  vi.advanceTimersByTime(GRACE_MS);
  expect(report).not.toHaveBeenCalled();
  expect(vi.getTimerCount()).toBe(action === "received" ? 1 : 0);
});

it("should not mistake a wall-clock jump for a stable connection", () => {
  const window = new StreamRecoveryWindow();
  window.opened(vi.fn());
  window.received(vi.fn());
  vi.setSystemTime(new Date("2030-01-01"));
  expect(window.isStable()).toBe(false);
  expect(window.isStale()).toBe(false);
});

it("should time out a connection attempt that never opens or errors", () => {
  const window = new StreamRecoveryWindow();
  const timeout = vi.fn();
  window.connecting(timeout);
  vi.advanceTimersByTime(STABLE_MS - 1);
  expect(timeout).not.toHaveBeenCalled();
  vi.advanceTimersByTime(1);
  expect(timeout).toHaveBeenCalledTimes(1);
  expect(window.isStale()).toBe(true);
  expect(vi.getTimerCount()).toBe(0);
});

it.each(["opened", "dispose", "reportLoss"] as const)("should cancel an attempt timeout on %s", (action) => {
  const window = new StreamRecoveryWindow();
  const timeout = vi.fn();
  window.connecting(timeout);
  window[action](vi.fn());
  vi.advanceTimersByTime(STABLE_MS);
  expect(timeout).not.toHaveBeenCalled();
  expect(vi.getTimerCount()).toBe(action === "opened" ? 1 : 0);
});

it("should replace an attempt timeout without retaining its callback", () => {
  const window = new StreamRecoveryWindow();
  const abandoned = vi.fn();
  const current = vi.fn();
  window.connecting(abandoned);
  window.connecting(current);
  expect(vi.getTimerCount()).toBe(1);
  vi.advanceTimersByTime(STABLE_MS);
  expect(abandoned).not.toHaveBeenCalled();
  expect(current).toHaveBeenCalledTimes(1);
});

it("should refresh silence expiry on actual delivery with only one timer", () => {
  const window = new StreamRecoveryWindow();
  const expired = vi.fn();
  window.opened(expired);
  vi.advanceTimersByTime(SILENCE_MS - 1);
  expect(window.isStale()).toBe(false);
  window.received(expired);
  vi.advanceTimersByTime(SILENCE_MS - 1);
  expect(expired).not.toHaveBeenCalled();
  expect(vi.getTimerCount()).toBe(1);
  vi.advanceTimersByTime(1);
  expect(expired).toHaveBeenCalledTimes(1);
  expect(window.isStale()).toBe(true);
  expect(vi.getTimerCount()).toBe(0);
});

it("should clear an open-connection deadline on teardown", () => {
  const window = new StreamRecoveryWindow();
  const expired = vi.fn();
  window.opened(expired);
  window.dispose();
  vi.advanceTimersByTime(SILENCE_MS);
  expect(expired).not.toHaveBeenCalled();
  expect(window.isStale()).toBe(false);
  expect(vi.getTimerCount()).toBe(0);
});
