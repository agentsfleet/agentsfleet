import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { FakeEventSource } from "@/tests/helpers/fake-event-source";
import { FRAME_KIND } from "@/lib/api/events-types";
import { __resetRegistryForTests, CONNECTION_STATUS, getSnapshot, subscribe } from "./fleet-stream-registry";

const WORKSPACE = "silence-workspace";
const FLEET = "silence-fleet";
const SILENCE_MS = 45_000;
const FIRST_RETRY_MS = 1_000;

function current() {
  const source = FakeEventSource.instances.at(-1);
  if (!source) throw new Error("Expected a stream");
  return source;
}

beforeEach(() => {
  vi.useFakeTimers();
  FakeEventSource.install();
  vi.stubGlobal("fetch", vi.fn(async () => Response.json({ items: [], next_cursor: null })));
  vi.spyOn(Math, "random").mockReturnValue(0);
});

afterEach(() => {
  __resetRegistryForTests();
  FakeEventSource.uninstall();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
  vi.useRealTimers();
});

it("should never call an all-day sequence of open-but-empty responses live", async () => {
  subscribe(WORKSPACE, FLEET, [], () => {});
  // pin test: the rejected age-only design falsely stayed LIVE indefinitely.
  for (let attempt = 0; attempt < 1_152; attempt += 1) {
    const source = current();
    source.open();
    expect(getSnapshot(FLEET).connectionStatus).not.toBe(CONNECTION_STATUS.LIVE);
    await vi.advanceTimersByTimeAsync(SILENCE_MS);
    expect(source.closed).toBe(true);
    if (attempt >= 5) {
      expect(getSnapshot(FLEET).connectionStatus).toBe(CONNECTION_STATUS.OFFLINE);
    }
    await vi.advanceTimersToNextTimerAsync();
    expect(current()).not.toBe(source);
    expect(FakeEventSource.instances.filter((stream) => !stream.closed)).toHaveLength(1);
  }
  current().open();
  expect(getSnapshot(FLEET).connectionStatus).toBe(CONNECTION_STATUS.OFFLINE);
  current().heartbeat();
  expect(getSnapshot(FLEET).connectionStatus).toBe(CONNECTION_STATUS.LIVE);
});

it("should not let malformed frames postpone silence detection", async () => {
  subscribe(WORKSPACE, FLEET, [], () => {});
  const source = current();
  source.open();
  for (const data of ["not json", "null", "42", "{}", '{"kind":42}']) {
    await vi.advanceTimersByTimeAsync(8_000);
    source.emitRaw(data);
  }
  expect(getSnapshot(FLEET).connectionStatus).toBe(CONNECTION_STATUS.CONNECTING);
  await vi.advanceTimersByTimeAsync(5_000);
  expect(source.closed).toBe(true);
  expect(getSnapshot(FLEET).connectionStatus).toBe(CONNECTION_STATUS.RECONNECTING);
});

it("should accept application traffic as liveness when it suppresses idle heartbeats", async () => {
  subscribe(WORKSPACE, FLEET, [], () => {});
  const source = current();
  source.open();
  for (let frame = 0; frame < 100; frame += 1) {
    await vi.advanceTimersByTimeAsync(40_000);
    source.emit({ kind: FRAME_KIND.EVENT_RECEIVED, event_id: `event-${frame}`, actor: "fleet" });
    expect(source.closed).toBe(false);
    expect(getSnapshot(FLEET).connectionStatus).toBe(CONNECTION_STATUS.LIVE);
  }
  expect(FakeEventSource.instances).toHaveLength(1);
  expect(fetch).not.toHaveBeenCalled();
  expect(vi.getTimerCount()).toBe(1);
});

it("should keep heartbeat work independent of subscriber count and fleet failures isolated", async () => {
  const listeners = Array.from({ length: 100 }, () => vi.fn());
  listeners.forEach((listener) => subscribe(WORKSPACE, FLEET, [], listener));
  const shared = current();
  subscribe(WORKSPACE, "second-fleet", [], () => {});
  const second = current();
  shared.open();
  shared.heartbeat();
  second.open();
  second.heartbeat();
  listeners.forEach((listener) => listener.mockClear());
  for (let beat = 0; beat < 100; beat += 1) {
    await vi.advanceTimersByTimeAsync(15_000);
    shared.heartbeat();
    second.heartbeat();
  }
  expect(listeners.every((listener) => listener.mock.calls.length === 0)).toBe(true);
  expect(FakeEventSource.instances).toHaveLength(2);
  expect(vi.getTimerCount()).toBe(2);
  second.fail();
  await vi.advanceTimersByTimeAsync(FIRST_RETRY_MS);
  expect(shared.closed).toBe(false);
  expect(current().url).toContain("second-fleet");
  expect(FakeEventSource.instances).toHaveLength(3);
  expect(listeners.every((listener) => listener.mock.calls.length === 0)).toBe(true);
  __resetRegistryForTests();
  expect(FakeEventSource.instances.every((stream) => stream.closed)).toBe(true);
  expect(vi.getTimerCount()).toBe(0);
});

it("should replace stale transport on foreground before suspended timers run", () => {
  subscribe(WORKSPACE, FLEET, [], () => {});
  const old = current();
  old.open();
  old.heartbeat();
  const now = performance.now();
  // Model browser suspension: monotonic time advances but timer tasks do not run.
  vi.spyOn(performance, "now").mockReturnValue(now + SILENCE_MS);
  document.dispatchEvent(new Event("visibilitychange"));
  window.dispatchEvent(new Event("online"));
  expect(old.closed).toBe(true);
  expect(FakeEventSource.instances).toHaveLength(2);
  expect(vi.getTimerCount()).toBe(1);
  current().open();
  current().heartbeat();
  old.heartbeat();
  old.fail();
  expect(current().closed).toBe(false);
  expect(getSnapshot(FLEET).connectionStatus).toBe(CONNECTION_STATUS.LIVE);
});
