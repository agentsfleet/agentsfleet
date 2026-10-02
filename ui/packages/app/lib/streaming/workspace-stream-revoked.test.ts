import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { FakeEventSource } from "@/tests/helpers/fake-event-source";
import {
  __resetWorkspaceRegistryForTests,
  getWorkspaceConnectionStatus,
  subscribeFleet,
  subscribeStatus,
  WORKSPACE_CONNECTION_STATUS,
} from "./workspace-stream";

const WS = "ws_1";
const FLEET_A = "z_a";
// Past the capped backoff: a reconnect would have opened a second stream.
const PAST_EVERY_RETRY_MS = 60_000;

function onlySource(): FakeEventSource {
  const [source, ...rest] = FakeEventSource.instances;
  if (!source || rest.length > 0) throw new Error(`expected one stream, found ${FakeEventSource.instances.length}`);
  return source;
}

beforeEach(() => {
  vi.useFakeTimers();
  FakeEventSource.install();
  __resetWorkspaceRegistryForTests();
});

afterEach(() => {
  __resetWorkspaceRegistryForTests();
  vi.useRealTimers();
  FakeEventSource.uninstall();
});

// The wall's one stream ends with `access_revoked` for a removed member, and
// the daemon refuses the next request the same way.
describe("workspace-stream — access revoked", () => {
  it("should close the wall's stream, never reconnect, and tell its status listeners", async () => {
    const statuses: string[] = [];
    subscribeFleet(WS, FLEET_A, () => {});
    subscribeStatus(WS, (status) => statuses.push(status));
    const source = onlySource();
    source.open();

    source.revokeAccess();
    source.fail();
    await vi.advanceTimersByTimeAsync(PAST_EVERY_RETRY_MS);

    expect(source.closed).toBe(true);
    expect(FakeEventSource.instances).toHaveLength(1);
    expect(getWorkspaceConnectionStatus(WS)).toBe(WORKSPACE_CONNECTION_STATUS.REVOKED);
    expect(statuses.at(-1)).toBe(WORKSPACE_CONNECTION_STATUS.REVOKED);
  });

  it("should tell a status listener that joins afterwards, without opening another stream", () => {
    subscribeFleet(WS, FLEET_A, () => {});
    onlySource().revokeAccess();

    const late = vi.fn();
    subscribeStatus(WS, late);

    expect(late).toHaveBeenCalledExactlyOnceWith(WORKSPACE_CONNECTION_STATUS.REVOKED);
    expect(FakeEventSource.instances).toHaveLength(1);
  });

  it("should not let a stream it already replaced end the one that took over", async () => {
    subscribeFleet(WS, FLEET_A, () => {});
    const replaced = onlySource();
    replaced.open();
    replaced.fail();
    await vi.advanceTimersByTimeAsync(PAST_EVERY_RETRY_MS);
    const current = FakeEventSource.instances.at(-1);
    if (!current || current === replaced) throw new Error("the wall's stream did not reconnect");
    current.open();

    replaced.revokeAccess();
    replaced.fail();

    expect(current.closed).toBe(false);
    expect(FakeEventSource.instances).toHaveLength(2);
    expect(getWorkspaceConnectionStatus(WS)).toBe(WORKSPACE_CONNECTION_STATUS.LIVE);
  });
});

// An owner who removes a teammate can invite them back. The wall they return to
// inside the idle grace is a new mount, and it must ask again.
describe("workspace-stream — access restored", () => {
  it("should open a fresh stream when a wall mounts on a revoked entry inside the idle grace", () => {
    const release = subscribeFleet(WS, FLEET_A, () => {});
    onlySource().revokeAccess();
    release();

    const statuses: string[] = [];
    subscribeStatus(WS, (status) => statuses.push(status));
    expect(statuses).toEqual([WORKSPACE_CONNECTION_STATUS.CONNECTING]);
    expect(FakeEventSource.instances).toHaveLength(2);
    FakeEventSource.instances.at(-1)?.open();

    expect(getWorkspaceConnectionStatus(WS)).toBe(WORKSPACE_CONNECTION_STATUS.LIVE);
  });

  it("should settle back on revoked, with no retry, when the caller is still removed", async () => {
    const release = subscribeFleet(WS, FLEET_A, () => {});
    onlySource().revokeAccess();
    release();

    subscribeFleet(WS, FLEET_A, () => {});
    FakeEventSource.instances.at(-1)?.revokeAccess();
    await vi.advanceTimersByTimeAsync(PAST_EVERY_RETRY_MS);

    expect(FakeEventSource.instances).toHaveLength(2);
    expect(getWorkspaceConnectionStatus(WS)).toBe(WORKSPACE_CONNECTION_STATUS.REVOKED);
  });
});
