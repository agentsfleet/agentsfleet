import { describe, expect, it, vi } from "vitest";
import { CONNECTION_STATUS, getSnapshot, subscribe } from "./fleet-stream-registry";
import { FRAME_KIND } from "@/lib/api/events-types";
import { FakeEventSource } from "@/tests/helpers/fake-event-source";
import { setupRegistryTests, WS, Z_A, NO_SEED, IDLE_RELEASE_MS, sourceAt } from "@/tests/helpers/fleet-stream-registry-fixtures";

setupRegistryTests();

describe("fleet-stream-registry — a lost connection recovers itself", () => {
  function failCurrent(): void {
    const es = sourceAt(-1);
    es.fail();
  }

  // Drives past the fast attempts so the connection is reported as not live.
  function exhaustFastAttempts(): void {
    for (let attempt = 0; attempt < FAST_ATTEMPTS + 1; attempt += 1) {
      failCurrent();
      vi.advanceTimersByTime(FAST_BACKOFF_CAP_MS);
    }
  }

  const FAST_ATTEMPTS = 5;
  const FAST_BACKOFF_CAP_MS = 15_000;
  const OFFLINE_RETRY_MS = 30_000;

  it("escalates an accept-then-close upstream instead of hammering at base delay", () => {
    // An unhealthy upstream that opens then immediately errors must NOT reset
    // the attempt count on open — otherwise it retries at the base delay
    // forever, stampeding the failing server. An immediate close proves
    // neither delivered data nor a stable connection window.
    const release = subscribe(WS, Z_A, NO_SEED, () => {});
    for (let cycle = 0; cycle < FAST_ATTEMPTS + 1; cycle += 1) {
      const es = sourceAt(-1);
      es.open();
      es.fail();
      vi.advanceTimersByTime(FAST_BACKOFF_CAP_MS);
    }
    // Despite every cycle reaching onopen, the connection is reported not live.
    expect(getSnapshot(Z_A).connectionStatus).toBe(CONNECTION_STATUS.OFFLINE);
    release();
  });

  it("escalates a one-frame-then-close upstream instead of resetting backoff forever", () => {
    const release = subscribe(WS, Z_A, NO_SEED, () => {});
    for (let cycle = 0; cycle < FAST_ATTEMPTS + 1; cycle += 1) {
      const es = sourceAt(-1);
      es.open();
      es.emit({ kind: FRAME_KIND.EVENT_RECEIVED, event_id: `frame-${cycle}`, actor: "fleet" });
      es.fail();
      vi.advanceTimersByTime(FAST_BACKOFF_CAP_MS);
    }
    expect(getSnapshot(Z_A).connectionStatus).toBe(CONNECTION_STATUS.OFFLINE);
    release();
  });

  it("returns to fast backoff once arrivals span the stable window", () => {
    const release = subscribe(WS, Z_A, NO_SEED, () => {});
    exhaustFastAttempts();
    expect(getSnapshot(Z_A).connectionStatus).toBe(CONNECTION_STATUS.OFFLINE);

    // Recover, then prove the connection is stable with arrivals 30 s apart.
    vi.advanceTimersByTime(OFFLINE_RETRY_MS);
    const es = sourceAt(-1);
    es.open();
    es.emit({
      kind: FRAME_KIND.EVENT_RECEIVED,
      event_id: "e1",
      actor: "fleet",
    });
    vi.advanceTimersByTime(30_000);
    es.heartbeat();
    // A subsequent failure is treated as attempt 1 after the stability window.
    const opened = FakeEventSource.instances.length;
    es.fail();
    vi.advanceTimersByTime(FAST_BACKOFF_CAP_MS);
    expect(FakeEventSource.instances.length).toBe(opened + 1);
    release();
  });

  it("keeps trying on its own once the fast attempts are exhausted", () => {
    const release = subscribe(WS, Z_A, NO_SEED, () => {});
    exhaustFastAttempts();
    expect(getSnapshot(Z_A).connectionStatus).toBe(CONNECTION_STATUS.OFFLINE);

    // No operator action of any kind — only time passing.
    const before = FakeEventSource.instances.length;
    vi.advanceTimersByTime(OFFLINE_RETRY_MS);
    expect(FakeEventSource.instances.length).toBe(before + 1);
    release();
  });

  it("reports not-live without ever abandoning the fleet", () => {
    const release = subscribe(WS, Z_A, NO_SEED, () => {});
    exhaustFastAttempts();
    const opened = FakeEventSource.instances.length;

    // Each unhurried attempt that also fails schedules the next one. The old
    // client stopped after a fixed count and only a button brought it back.
    for (let round = 0; round < 3; round += 1) {
      failCurrent();
      vi.advanceTimersByTime(OFFLINE_RETRY_MS);
    }
    expect(FakeEventSource.instances.length).toBe(opened + 3);
    expect(getSnapshot(Z_A).connectionStatus).toBe(CONNECTION_STATUS.OFFLINE);
    release();
  });

  it("retries immediately when the tab returns or the network comes back", () => {
    const release = subscribe(WS, Z_A, NO_SEED, () => {});
    exhaustFastAttempts();
    const opened = FakeEventSource.instances.length;

    document.dispatchEvent(new Event("visibilitychange"));
    expect(FakeEventSource.instances.length).toBe(opened + 1);
    expect(getSnapshot(Z_A).connectionStatus).toBe(CONNECTION_STATUS.CONNECTING);
    release();
  });

  it("opens exactly one connection when both recovery signals fire together", () => {
    const release = subscribe(WS, Z_A, NO_SEED, () => {});
    exhaustFastAttempts();
    const opened = FakeEventSource.instances.length;

    document.dispatchEvent(new Event("visibilitychange"));
    window.dispatchEvent(new Event("online"));
    // The second signal finds a connection already in flight and does nothing.
    expect(FakeEventSource.instances.length).toBe(opened + 1);
    release();
  });

  it("does not reconnect for a tab that is still hidden", () => {
    const release = subscribe(WS, Z_A, NO_SEED, () => {});
    exhaustFastAttempts();
    const opened = FakeEventSource.instances.length;

    // `visibilitychange` fires on the way OUT as well as in. Reconnecting for a
    // tab nobody is looking at spends a stream slot on nothing.
    const visibility = vi.spyOn(document, "visibilityState", "get").mockReturnValue("hidden");
    document.dispatchEvent(new Event("visibilitychange"));
    expect(FakeEventSource.instances.length).toBe(opened);

    visibility.mockReturnValue("visible");
    document.dispatchEvent(new Event("visibilitychange"));
    expect(FakeEventSource.instances.length).toBe(opened + 1);
    visibility.mockRestore();
    release();
  });

  it("ignores a recovery signal while a connection is already live", () => {
    const release = subscribe(WS, Z_A, NO_SEED, () => {});
    const opened = FakeEventSource.instances.length;
    window.dispatchEvent(new Event("online"));
    expect(FakeEventSource.instances.length).toBe(opened);
    release();
  });

  it("stops listening for recovery once the fleet's last subscriber is gone", () => {
    const release = subscribe(WS, Z_A, NO_SEED, () => {});
    exhaustFastAttempts();
    release();
    vi.advanceTimersByTime(IDLE_RELEASE_MS);
    const afterTeardown = FakeEventSource.instances.length;

    window.dispatchEvent(new Event("online"));
    document.dispatchEvent(new Event("visibilitychange"));
    expect(FakeEventSource.instances.length).toBe(afterTeardown);
  });
});
