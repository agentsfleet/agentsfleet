import { describe, expect, it, vi } from "vitest";
import { CONNECTION_STATUS, getSnapshot, retryConnection, subscribe } from "./fleet-stream-registry";
import { OFFLINE_RETRY_MS } from "./fleet-stream-reconnect";
import { FakeEventSource } from "@/tests/helpers/fake-event-source";
import { NO_SEED, WS, Z_A, setupRegistryTests, sourceAt } from "@/tests/helpers/fleet-stream-registry-fixtures";

// Past every fast attempt and one unhurried retry: a reconnect would have
// opened a second stream inside this.
const PAST_EVERY_RETRY_MS = OFFLINE_RETRY_MS * 2;

setupRegistryTests();

// A removed member's stream ends with `access_revoked`, and the daemon refuses
// the next request the same way, so knocking again only spends requests.
describe("fleet-stream-registry — access revoked", () => {
  it("should close the stream, never reconnect, and report that access is gone", async () => {
    const release = subscribe(WS, Z_A, NO_SEED, () => {});
    const source = sourceAt(0);
    source.open();

    source.revokeAccess();
    source.fail();
    await vi.advanceTimersByTimeAsync(PAST_EVERY_RETRY_MS);

    expect(source.closed).toBe(true);
    expect(FakeEventSource.instances).toHaveLength(1);
    expect(getSnapshot(Z_A).connectionStatus).toBe(CONNECTION_STATUS.REVOKED);
    release();
  });

  it("should open nothing for the tab returning, the network returning, or the operator's retry", async () => {
    const release = subscribe(WS, Z_A, NO_SEED, () => {});
    sourceAt(0).open();
    sourceAt(0).revokeAccess();

    document.dispatchEvent(new Event("visibilitychange"));
    window.dispatchEvent(new Event("online"));
    retryConnection(Z_A);
    await vi.advanceTimersByTimeAsync(PAST_EVERY_RETRY_MS);

    expect(FakeEventSource.instances).toHaveLength(1);
    expect(getSnapshot(Z_A).connectionStatus).toBe(CONNECTION_STATUS.REVOKED);
    release();
  });

  it("should not let a stream it already replaced end the one that took over", async () => {
    const release = subscribe(WS, Z_A, NO_SEED, () => {});
    const replaced = sourceAt(0);
    replaced.open();
    replaced.fail();
    await vi.advanceTimersByTimeAsync(OFFLINE_RETRY_MS);
    const current = sourceAt(1);
    current.open();
    current.heartbeat();

    replaced.revokeAccess();

    expect(current.closed).toBe(false);
    expect(getSnapshot(Z_A).connectionStatus).toBe(CONNECTION_STATUS.LIVE);
    release();
  });
});

// An owner who removes a teammate can invite them back. The page they return to
// is a new mount, and it must ask again rather than inherit the closed stream.
describe("fleet-stream-registry — access restored", () => {
  it("should open a fresh stream when a view mounts on a revoked entry inside the idle window", async () => {
    const releaseFirst = subscribe(WS, Z_A, NO_SEED, () => {});
    sourceAt(0).open();
    sourceAt(0).revokeAccess();
    releaseFirst();

    const releaseSecond = subscribe(WS, Z_A, NO_SEED, () => {});
    expect(getSnapshot(Z_A).connectionStatus).toBe(CONNECTION_STATUS.CONNECTING);
    expect(FakeEventSource.instances).toHaveLength(2);
    sourceAt(1).open();
    sourceAt(1).heartbeat();

    expect(getSnapshot(Z_A).connectionStatus).toBe(CONNECTION_STATUS.LIVE);
    releaseSecond();
  });

  it("should restore the network-online signal, so the reopened stream recovers like a new one", async () => {
    const releaseFirst = subscribe(WS, Z_A, NO_SEED, () => {});
    sourceAt(0).open();
    sourceAt(0).revokeAccess();
    releaseFirst();
    const releaseSecond = subscribe(WS, Z_A, NO_SEED, () => {});
    sourceAt(1).open();
    sourceAt(1).fail();

    window.dispatchEvent(new Event("online"));

    expect(FakeEventSource.instances).toHaveLength(3);
    releaseSecond();
  });

  it("should settle back on revoked, with no retry, when the caller is still removed", async () => {
    const releaseFirst = subscribe(WS, Z_A, NO_SEED, () => {});
    sourceAt(0).open();
    sourceAt(0).revokeAccess();
    releaseFirst();

    const releaseSecond = subscribe(WS, Z_A, NO_SEED, () => {});
    sourceAt(1).revokeAccess();
    await vi.advanceTimersByTimeAsync(PAST_EVERY_RETRY_MS);

    expect(FakeEventSource.instances).toHaveLength(2);
    expect(getSnapshot(Z_A).connectionStatus).toBe(CONNECTION_STATUS.REVOKED);
    releaseSecond();
  });

  it("should not reopen for a second listener while the stream is live", () => {
    const releaseFirst = subscribe(WS, Z_A, NO_SEED, () => {});
    sourceAt(0).open();
    const releaseSecond = subscribe(WS, Z_A, NO_SEED, () => {});

    expect(FakeEventSource.instances).toHaveLength(1);
    releaseFirst();
    releaseSecond();
  });
});
