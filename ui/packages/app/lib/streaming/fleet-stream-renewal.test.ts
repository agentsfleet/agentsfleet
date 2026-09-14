// Vitest is the app's coverage runner; approved for this change.
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { FakeEventSource } from "@/tests/helpers/fake-event-source";
import { FRAME_KIND } from "@/lib/api/events-types";
import { __resetRegistryForTests, CONNECTION_STATUS, getSnapshot, retryConnection, subscribe } from "./fleet-stream-registry";

const WORKSPACE = "renewal-workspace";
const FLEET = "renewal-fleet";
const ROTATION_MS = 300_000;
const RETRY_CEILING_MS = 2_000;
const GRACE_MS = 25_000;
const SLOW_RETRY_MS = 30_000;
const HEARTBEAT_INTERVAL_MS = 15_000;

function current() {
  const source = FakeEventSource.instances.at(-1);
  if (!source) throw new Error("Expected a stream");
  return source;
}

async function quietTraffic(duration: number) {
  current().heartbeat();
  for (let elapsed = 0; elapsed < duration; elapsed += HEARTBEAT_INTERVAL_MS) {
    await vi.advanceTimersByTimeAsync(HEARTBEAT_INTERVAL_MS);
    current().heartbeat();
  }
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

it("should renew an all-day quiet chat without accumulating failures or flashing status", async () => {
  const listener = vi.fn();
  subscribe(WORKSPACE, FLEET, [], listener);
  current().open();
  // pin test: 288 five-minute renewals represent an entire day.
  for (let rotation = 0; rotation < 288; rotation += 1) {
    await quietTraffic(ROTATION_MS);
    listener.mockClear();
    current().fail();
    expect(getSnapshot(FLEET).connectionStatus).toBe(CONNECTION_STATUS.LIVE);
    expect(listener).not.toHaveBeenCalled();
    const count = FakeEventSource.instances.length;
    await vi.advanceTimersByTimeAsync(RETRY_CEILING_MS);
    expect(FakeEventSource.instances).toHaveLength(count + 1);
    await vi.advanceTimersByTimeAsync(HEARTBEAT_INTERVAL_MS);
    current().open();
    current().heartbeat();
    await vi.advanceTimersByTimeAsync(0);
    expect(getSnapshot(FLEET).connectionStatus).toBe(CONNECTION_STATUS.LIVE);
    expect(FakeEventSource.instances.filter((source) => !source.closed)).toHaveLength(1);
  }
  expect(fetch).toHaveBeenCalledTimes(288);
});

it("should ignore stale errors, opens and messages after manual replacement", () => {
  subscribe(WORKSPACE, FLEET, [], () => {});
  const old = current();
  retryConnection(FLEET);
  current().open();
  current().heartbeat();
  old.fail();
  old.open();
  old.heartbeat();
  old.emit({ kind: FRAME_KIND.EVENT_RECEIVED, event_id: "stale", actor: "fleet" });
  old.emitRaw(JSON.stringify({ kind: FRAME_KIND.EVENT_RECEIVED, event_id: "stale", actor: "fleet" }));
  expect(current().closed).toBe(false);
  expect(getSnapshot(FLEET).connectionStatus).toBe(CONNECTION_STATUS.LIVE);
  expect(getSnapshot(FLEET).events).toEqual([]);
  expect(fetch).not.toHaveBeenCalled();
});

it("should surface sustained failure after grace and keep retrying without user action", async () => {
  subscribe(WORKSPACE, FLEET, [], () => {});
  current().open();
  await quietTraffic(ROTATION_MS);
  current().fail();
  await vi.advanceTimersByTimeAsync(GRACE_MS - 1);
  expect(getSnapshot(FLEET).connectionStatus).toBe(CONNECTION_STATUS.LIVE);
  await vi.advanceTimersByTimeAsync(1);
  expect(getSnapshot(FLEET).connectionStatus).toBe(CONNECTION_STATUS.RECONNECTING);
  for (let attempt = 0; attempt < 5; attempt += 1) {
    current().fail();
    await vi.advanceTimersByTimeAsync(SLOW_RETRY_MS);
  }
  expect(getSnapshot(FLEET).connectionStatus).toBe(CONNECTION_STATUS.OFFLINE);
  const count = FakeEventSource.instances.length;
  current().fail();
  await vi.advanceTimersByTimeAsync(SLOW_RETRY_MS);
  expect(FakeEventSource.instances).toHaveLength(count + 1);
  current().open();
  current().heartbeat();
  expect(getSnapshot(FLEET).connectionStatus).toBe(CONNECTION_STATUS.LIVE);
});

it("should publish the current failure level when attempts exhaust inside grace", async () => {
  subscribe(WORKSPACE, FLEET, [], () => {});
  current().open();
  await quietTraffic(ROTATION_MS);
  // pin test: equal-jitter minima put six failures within the 25s grace.
  const minimumRetryDelays = [1_000, 2_000, 4_000, 7_500, 7_500];
  for (const delay of minimumRetryDelays) {
    current().fail();
    await vi.advanceTimersByTimeAsync(delay);
  }
  current().fail();
  expect(getSnapshot(FLEET).connectionStatus).toBe(CONNECTION_STATUS.LIVE);
  await vi.advanceTimersByTimeAsync(3_000);
  expect(getSnapshot(FLEET).connectionStatus).toBe(CONNECTION_STATUS.OFFLINE);
});

it("should keep one replacement when recovery signals overlap a queued retry", async () => {
  const releases = Array.from({ length: 100 }, () => subscribe(WORKSPACE, FLEET, [], () => {}));
  expect(FakeEventSource.instances).toHaveLength(1);
  current().open();
  await quietTraffic(ROTATION_MS);
  const old = current();
  old.fail();
  window.dispatchEvent(new Event("online"));
  document.dispatchEvent(new Event("visibilitychange"));
  old.fail();
  expect(getSnapshot(FLEET).connectionStatus).toBe(CONNECTION_STATUS.LIVE);
  await vi.advanceTimersByTimeAsync(RETRY_CEILING_MS);
  expect(FakeEventSource.instances).toHaveLength(2);
  expect(current().closed).toBe(false);
  current().open();
  current().heartbeat();
  await vi.advanceTimersByTimeAsync(GRACE_MS);
  expect(getSnapshot(FLEET).connectionStatus).toBe(CONNECTION_STATUS.LIVE);
  releases.forEach((release) => release());
  await vi.advanceTimersByTimeAsync(SLOW_RETRY_MS);
  expect(current().closed).toBe(true);
  expect(vi.getTimerCount()).toBe(0);
});

it("should preserve the original grace deadline when tab wake accelerates recovery", async () => {
  subscribe(WORKSPACE, FLEET, [], () => {});
  current().open();
  await quietTraffic(ROTATION_MS);
  current().fail();
  await vi.advanceTimersByTimeAsync(500);
  window.dispatchEvent(new Event("online"));
  expect(getSnapshot(FLEET).connectionStatus).toBe(CONNECTION_STATUS.LIVE);
  await vi.advanceTimersByTimeAsync(GRACE_MS - 500);
  expect(getSnapshot(FLEET).connectionStatus).toBe(CONNECTION_STATUS.RECONNECTING);
});

it("should reject every callback from a torn-down entry after the fleet is resubscribed", () => {
  subscribe(WORKSPACE, FLEET, [], () => {});
  const old = current();
  __resetRegistryForTests();
  const listener = vi.fn();
  subscribe(WORKSPACE, FLEET, [], listener);
  old.open();
  old.fail();
  old.heartbeat();
  old.emit({ kind: FRAME_KIND.EVENT_RECEIVED, event_id: "stale", actor: "fleet" });
  expect(listener).not.toHaveBeenCalled();
  expect(current().closed).toBe(false);
  expect(fetch).not.toHaveBeenCalled();
  expect(vi.getTimerCount()).toBe(1);
});

it("should retry blackholed connection attempts and eventually offer manual recovery", async () => {
  subscribe(WORKSPACE, FLEET, [], () => {});
  for (let attempt = 0; attempt < 6; attempt += 1) {
    const source = current();
    await vi.advanceTimersByTimeAsync(SLOW_RETRY_MS);
    expect(source.closed).toBe(true);
    await vi.advanceTimersToNextTimerAsync();
    expect(current()).not.toBe(source);
  }
  expect(getSnapshot(FLEET).connectionStatus).toBe(CONNECTION_STATUS.OFFLINE);
  retryConnection(FLEET);
  current().open();
  current().heartbeat();
  await vi.advanceTimersByTimeAsync(SLOW_RETRY_MS);
  expect(getSnapshot(FLEET).connectionStatus).toBe(CONNECTION_STATUS.LIVE);
  expect(vi.getTimerCount()).toBe(1);
});

it("should quietly replace a formerly healthy silent socket and ignore its late callbacks", async () => {
  subscribe(WORKSPACE, FLEET, [], () => {});
  const old = current();
  old.open();
  await quietTraffic(30_000);
  // pin test: three missed 15-second keepalives make the transport stale.
  await vi.advanceTimersByTimeAsync(45_000);
  expect(old.closed).toBe(true);
  expect(getSnapshot(FLEET).connectionStatus).toBe(CONNECTION_STATUS.LIVE);
  await vi.advanceTimersByTimeAsync(RETRY_CEILING_MS);
  current().open();
  current().heartbeat();
  old.open();
  old.fail();
  old.heartbeat();
  expect(current().closed).toBe(false);
  expect(getSnapshot(FLEET).connectionStatus).toBe(CONNECTION_STATUS.LIVE);
  await vi.advanceTimersByTimeAsync(GRACE_MS);
  expect(fetch).toHaveBeenCalledTimes(1);
  expect(vi.getTimerCount()).toBe(1);
  __resetRegistryForTests();
  expect(vi.getTimerCount()).toBe(0);
});
