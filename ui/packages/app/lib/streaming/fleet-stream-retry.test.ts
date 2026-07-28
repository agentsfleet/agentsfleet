import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  __resetRegistryForTests,
  CONNECTION_STATUS,
  getSnapshot,
  retryConnection,
  subscribe,
} from "./fleet-stream-registry";
import { FakeEventSource } from "@/tests/helpers/fake-event-source";

const FLEET_ID = "fleet_retry";

beforeEach(() => {
  vi.useFakeTimers();
  FakeEventSource.install();
  __resetRegistryForTests();
});

afterEach(() => {
  __resetRegistryForTests();
  vi.useRealTimers();
  FakeEventSource.uninstall();
});

function failCurrentConnection() {
  FakeEventSource.instances.at(-1)!.fail();
}

describe("fleet stream retry lifecycle", () => {
  it("ignores a manual retry for a fleet without a stream entry", () => {
    retryConnection("missing-fleet");
    expect(FakeEventSource.instances).toHaveLength(0);
  });

  it("deduplicates repeated errors from one connection", () => {
    const unsubscribe = subscribe("ws_1", FLEET_ID, [], () => {});
    const source = FakeEventSource.instances[0]!;
    source.fail();
    source.fail();
    vi.runOnlyPendingTimers();
    expect(FakeEventSource.instances).toHaveLength(2);
    unsubscribe();
  });

  it("stops automatic retries and lets the operator start fresh", () => {
    const unsubscribe = subscribe("ws_1", FLEET_ID, [], () => {});
    for (let attempt = 0; attempt < 6; attempt += 1) {
      failCurrentConnection();
      vi.runOnlyPendingTimers();
    }
    expect(getSnapshot(FLEET_ID).connectionStatus).toBe(CONNECTION_STATUS.OFFLINE);
    const beforeRetry = FakeEventSource.instances.length;
    retryConnection(FLEET_ID);
    expect(getSnapshot(FLEET_ID).connectionStatus).toBe(CONNECTION_STATUS.CONNECTING);
    expect(FakeEventSource.instances).toHaveLength(beforeRetry + 1);
    unsubscribe();
  });

  it("cancels a scheduled reconnect before retrying immediately", () => {
    const unsubscribe = subscribe("ws_1", FLEET_ID, [], () => {});
    failCurrentConnection();
    expect(getSnapshot(FLEET_ID).connectionStatus).toBe(CONNECTION_STATUS.RECONNECTING);
    retryConnection(FLEET_ID);
    vi.runOnlyPendingTimers();
    expect(FakeEventSource.instances).toHaveLength(2);
    unsubscribe();
  });
});
