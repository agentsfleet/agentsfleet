import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  __resetWorkspaceRegistryForTests,
  getWorkspaceConnectionStatus,
  noteServerFrameTime,
  parseWorkspaceFrame,
  subscribeFleet,
  subscribeStatus,
  subscribeWorkspaceFrames,
  WORKSPACE_CONNECTION_STATUS,
  type BackfillFn,
} from "./workspace-stream";
import { FRAME_KIND } from "@/lib/api/events";

// Mirrors the FakeEventSource pattern in fleet-stream-registry.test.ts.
class FakeEventSource {
  static instances: FakeEventSource[] = [];
  url: string;
  onopen: ((this: EventSource, ev: Event) => unknown) | null = null;
  onmessage: ((this: EventSource, ev: MessageEvent) => unknown) | null = null;
  onerror: ((this: EventSource, ev: Event) => unknown) | null = null;
  closed = false;
  constructor(url: string) {
    this.url = url;
    FakeEventSource.instances.push(this);
  }
  close() {
    this.closed = true;
  }
  emit(data: unknown) {
    this.onmessage?.call(this as unknown as EventSource, { data } as MessageEvent);
  }
  open() {
    this.onopen?.call(this as unknown as EventSource, {} as Event);
  }
  fail() {
    this.onerror?.call(this as unknown as EventSource, {} as Event);
  }
}

const WS = "ws_1";
const FLEET_A = "z_a";
const FLEET_B = "z_b";
const IDLE_RELEASE_MS = 30_000;

function frame(fleetId: string, over: Record<string, unknown> = {}): string {
  return JSON.stringify({ fleet_id: fleetId, kind: "event_received", event_id: "e1", actor: "a", ...over });
}

beforeEach(() => {
  vi.useFakeTimers();
  FakeEventSource.instances = [];
  (globalThis as unknown as { EventSource: unknown }).EventSource = FakeEventSource;
  __resetWorkspaceRegistryForTests();
});

afterEach(() => {
  __resetWorkspaceRegistryForTests();
  vi.useRealTimers();
  delete (globalThis as { EventSource?: unknown }).EventSource;
});

describe("workspace-stream — one connection, demultiplexed by fleet", () => {
  it("opens exactly one EventSource per workspace regardless of tile count", () => {
    const a = subscribeFleet(WS, FLEET_A, () => {});
    const b = subscribeFleet(WS, FLEET_B, () => {});
    expect(FakeEventSource.instances.length).toBe(1);
    a();
    b();
  });

  it("routes each tagged frame only to the tile that subscribed for that fleet", () => {
    const onA = vi.fn();
    const onB = vi.fn();
    subscribeFleet(WS, FLEET_A, onA);
    subscribeFleet(WS, FLEET_B, onB);
    const es = FakeEventSource.instances[0]!;
    es.open();

    es.emit(frame(FLEET_A));
    expect(onA).toHaveBeenCalledTimes(1);
    expect(onB).not.toHaveBeenCalled();
    expect(onA.mock.calls[0]![0].fleet_id).toBe(FLEET_A);

    es.emit(frame(FLEET_B));
    expect(onB).toHaveBeenCalledTimes(1);
    expect(onA).toHaveBeenCalledTimes(1); // A did not see B's frame
  });

  it("drops a malformed or untagged frame — never routes it to a wrong tile, never throws", () => {
    const onA = vi.fn();
    subscribeFleet(WS, FLEET_A, onA);
    const es = FakeEventSource.instances[0]!;
    es.open();

    es.emit("not json");
    es.emit({ kind: FRAME_KIND.EVENT_RECEIVED });
    es.emit(JSON.stringify({ kind: "event_received", event_id: "e" })); // no fleet_id
    es.emit(JSON.stringify({ fleet_id: FLEET_A })); // no kind
    es.emit(JSON.stringify({ fleet_id: "", kind: "event_received" })); // empty fleet_id
    es.emit(JSON.stringify(["array"]));
    expect(onA).not.toHaveBeenCalled();

    // A well-formed frame after the bad ones still routes.
    es.emit(frame(FLEET_A));
    expect(onA).toHaveBeenCalledTimes(1);
  });

  it("routes hello and catching_up to workspace listeners, never tile listeners", () => {
    const onFleet = vi.fn();
    const onWorkspace = vi.fn();
    subscribeFleet(WS, FLEET_A, onFleet);
    subscribeWorkspaceFrames(WS, onWorkspace);
    const es = FakeEventSource.instances[0]!;
    es.open();

    es.emit(JSON.stringify({ kind: FRAME_KIND.HELLO, fleet_ids: [FLEET_A, FLEET_B] }));
    es.emit(JSON.stringify({ kind: FRAME_KIND.CATCHING_UP, dropped: 2 }));

    expect(onFleet).not.toHaveBeenCalled();
    expect(onWorkspace).toHaveBeenCalledTimes(2);
    expect(onWorkspace.mock.calls[0]![0].kind).toBe(FRAME_KIND.HELLO);
    expect(onWorkspace.mock.calls[1]![0].kind).toBe(FRAME_KIND.CATCHING_UP);
  });

  it("a frame for a fleet no tile is watching is silently ignored", () => {
    const onA = vi.fn();
    subscribeFleet(WS, FLEET_A, onA);
    const es = FakeEventSource.instances[0]!;
    es.open();
    es.emit(frame("z_unwatched"));
    expect(onA).not.toHaveBeenCalled();
  });
});

describe("workspace-stream — reconnect + backfill", () => {
  it("reconnects with backoff after an error and backfills on the reconnect open, not the first", async () => {
    const backfill: BackfillFn = vi.fn(async () => {});
    subscribeFleet(WS, FLEET_A, () => {}, backfill);
    const first = FakeEventSource.instances[0]!;

    // First open must NOT backfill (SSR-seeded initial connect).
    first.open();
    expect(backfill).not.toHaveBeenCalled();

    // Error → schedule reconnect; a new EventSource opens after the backoff.
    first.fail();
    expect(first.closed).toBe(true);
    await vi.advanceTimersByTimeAsync(2_000);
    expect(FakeEventSource.instances.length).toBe(2);

    // The reconnect open DOES backfill the gap.
    const second = FakeEventSource.instances[1]!;
    second.open();
    await vi.runOnlyPendingTimersAsync();
    expect(backfill).toHaveBeenCalledTimes(1);
    expect(backfill).toHaveBeenCalledWith(WS, null);
  });

  it("surfaces connection status transitions for the wall's degraded eyebrow", () => {
    const statuses: string[] = [];
    subscribeStatus(WS, (s) => statuses.push(s));
    const es = FakeEventSource.instances[0]!;
    es.open();
    es.open();
    es.fail();
    // connecting (initial) → live (open) → reconnecting (error)
    expect(statuses).toContain(WORKSPACE_CONNECTION_STATUS.CONNECTING);
    expect(statuses).toContain(WORKSPACE_CONNECTION_STATUS.LIVE);
    expect(statuses.at(-1)).toBe(WORKSPACE_CONNECTION_STATUS.RECONNECTING);
  });

  it("starts workspace backfill when the server reports dropped frames", async () => {
    const backfill: BackfillFn = vi.fn(async () => {});
    subscribeWorkspaceFrames(WS, () => {}, backfill);
    const es = FakeEventSource.instances[0]!;
    es.open();

    es.emit(JSON.stringify({ kind: FRAME_KIND.CATCHING_UP, dropped: 2 }));
    await Promise.resolve();

    expect(backfill).toHaveBeenCalledTimes(1);
    expect(backfill).toHaveBeenCalledWith(WS, null);
  });
});

describe("workspace-stream — lifecycle", () => {
  it("reports connecting before a workspace has subscribers", () => {
    expect(getWorkspaceConnectionStatus(WS)).toBe(WORKSPACE_CONNECTION_STATUS.CONNECTING);
  });

  it("keeps the connection until the last subscriber leaves, then releases after the idle grace", () => {
    const a = subscribeFleet(WS, FLEET_A, () => {});
    const b = subscribeFleet(WS, FLEET_B, () => {});
    const es = FakeEventSource.instances[0]!;
    a();
    vi.advanceTimersByTime(IDLE_RELEASE_MS + 1);
    expect(es.closed).toBe(false); // b still holds it
    b();
    vi.advanceTimersByTime(IDLE_RELEASE_MS + 1);
    expect(es.closed).toBe(true);
  });

  it("keeps a shared fleet listener set until its final listener leaves", () => {
    const onFirst = vi.fn();
    const onSecond = vi.fn();
    const first = subscribeFleet(WS, FLEET_A, onFirst);
    const second = subscribeFleet(WS, FLEET_A, onSecond);
    const es = FakeEventSource.instances[0]!;

    first();
    es.emit(frame(FLEET_A));

    expect(onFirst).not.toHaveBeenCalled();
    expect(onSecond).toHaveBeenCalledTimes(1);
    second();
  });

  it("allows cleanup handles to run after registry teardown", () => {
    const fleet = subscribeFleet(WS, FLEET_A, () => {});
    const status = subscribeStatus(WS, () => {});
    const workspace = subscribeWorkspaceFrames(WS, () => {});

    __resetWorkspaceRegistryForTests();

    fleet();
    status();
    workspace();
  });

  it("a re-subscribe within the idle window reuses the same connection", () => {
    const a = subscribeFleet(WS, FLEET_A, () => {});
    a();
    vi.advanceTimersByTime(IDLE_RELEASE_MS - 1);
    const b = subscribeFleet(WS, FLEET_A, () => {});
    expect(FakeEventSource.instances.length).toBe(1);
    vi.advanceTimersByTime(IDLE_RELEASE_MS + 1);
    expect(FakeEventSource.instances[0]!.closed).toBe(false);
    b();
  });
});

describe("workspace-stream — server time", () => {
  it("keeps the newest server-confirmed backfill anchor", async () => {
    noteServerFrameTime(WS, 1);
    const backfill: BackfillFn = vi.fn(async () => {});
    subscribeFleet(WS, FLEET_A, () => {}, backfill);
    const first = FakeEventSource.instances[0]!;
    first.open();

    noteServerFrameTime(WS, 20);
    noteServerFrameTime(WS, 10);
    noteServerFrameTime(WS, 30);
    first.fail();
    await vi.advanceTimersByTimeAsync(2_000);
    FakeEventSource.instances[1]!.open();
    await Promise.resolve();

    expect(getWorkspaceConnectionStatus(WS)).toBe(WORKSPACE_CONNECTION_STATUS.LIVE);
    expect(backfill).toHaveBeenCalledWith(WS, 30);
  });
});

describe("parseWorkspaceFrame", () => {
  it("accepts a tagged object frame and rejects everything else", () => {
    expect(parseWorkspaceFrame(frame(FLEET_A))).toMatchObject({ fleet_id: FLEET_A });
    expect(parseWorkspaceFrame("{bad json")).toBeNull();
    expect(parseWorkspaceFrame("null")).toBeNull();
    expect(parseWorkspaceFrame(JSON.stringify({ kind: "x" }))).toBeNull();
    expect(parseWorkspaceFrame(JSON.stringify({ fleet_id: "z" }))).toBeNull();
  });

  it("accepts workspace control frames without fleet_id", () => {
    expect(parseWorkspaceFrame(JSON.stringify({ kind: FRAME_KIND.HELLO, fleet_ids: [FLEET_A] }))?.kind).toBe(
      FRAME_KIND.HELLO,
    );
    expect(parseWorkspaceFrame(JSON.stringify({ kind: FRAME_KIND.CATCHING_UP, dropped: 1 }))?.kind).toBe(
      FRAME_KIND.CATCHING_UP,
    );
    expect(parseWorkspaceFrame(JSON.stringify({ kind: FRAME_KIND.HELLO, fleet_ids: [1] }))).toBeNull();
    expect(parseWorkspaceFrame(JSON.stringify({ kind: FRAME_KIND.HELLO, fleet_ids: [""] }))).toBeNull();
    expect(parseWorkspaceFrame(JSON.stringify({ kind: FRAME_KIND.CATCHING_UP, dropped: "1" }))).toBeNull();
    expect(parseWorkspaceFrame(JSON.stringify({ kind: FRAME_KIND.CATCHING_UP, dropped: -1 }))).toBeNull();
    expect(parseWorkspaceFrame(JSON.stringify({ kind: FRAME_KIND.CATCHING_UP, dropped: 1.5 }))).toBeNull();
  });
});
