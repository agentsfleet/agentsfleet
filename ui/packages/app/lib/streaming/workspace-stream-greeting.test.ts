import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { FRAME_KIND } from "@/lib/api/events-types";
import { FakeEventSource } from "@/tests/helpers/fake-event-source";
import {
  __resetWorkspaceRegistryForTests,
  lastGreeting,
  subscribeWorkspaceFrames,
} from "./workspace-stream";

// The server greets on connect and again only when the set changes, so a wall
// that mounts onto a connection kept through the idle grace missed the hello.
// The registry keeps the newest set for it; these prove what it keeps and when
// it lets go.

const WS = "ws_greeting";
const FLEET_A = "z_a";
const FLEET_B = "z_b";
const RECONNECT_DELAY_MS = 2_000;
const COUNTERS = { [FLEET_A]: { events_processed: 5, budget_used_nanos: 1_000 } };

// The shared fake dispatches by frame name exactly as a browser does; a
// greeting is a named `hello` frame like any other.
function greet(source: FakeEventSource, frame: Record<string, unknown>): void {
  const data = JSON.stringify({ kind: FRAME_KIND.HELLO, ...frame });
  for (const listener of source.listeners.get(FRAME_KIND.HELLO) ?? []) {
    listener({ data } as MessageEvent);
  }
}

function connection(index: number): FakeEventSource {
  const source = FakeEventSource.instances[index];
  if (!source) throw new Error(`connection ${index} was never opened`);
  return source;
}

beforeEach(() => {
  vi.useFakeTimers();
  FakeEventSource.install();
  __resetWorkspaceRegistryForTests();
});

afterEach(() => {
  __resetWorkspaceRegistryForTests();
  FakeEventSource.uninstall();
  vi.useRealTimers();
});

describe("workspace-stream — the greeting a late wall takes", () => {
  it("keeps the newest announced set, without its counters", () => {
    subscribeWorkspaceFrames(WS, () => {});
    connection(0).open();
    greet(connection(0), { fleet_ids: [FLEET_A], counters: COUNTERS });
    greet(connection(0), { fleet_ids: [FLEET_A, FLEET_B], counters: COUNTERS });

    expect(lastGreeting(WS)).toEqual({ kind: FRAME_KIND.HELLO, fleet_ids: [FLEET_A, FLEET_B] });
  });

  it("holds nothing before the first hello, or for a workspace with no connection", () => {
    subscribeWorkspaceFrames(WS, () => {});
    connection(0).open();

    expect(lastGreeting(WS)).toBeNull();
    expect(lastGreeting("ws_unknown")).toBeNull();
  });

  it("lets go when the connection drops, and takes the reconnect's own hello", () => {
    subscribeWorkspaceFrames(WS, () => {});
    connection(0).open();
    greet(connection(0), { fleet_ids: [FLEET_A] });
    connection(0).fail();

    expect(lastGreeting(WS)).toBeNull();
    vi.advanceTimersByTime(RECONNECT_DELAY_MS);
    connection(1).open();
    greet(connection(1), { fleet_ids: [FLEET_B] });
    expect(lastGreeting(WS)).toEqual({ kind: FRAME_KIND.HELLO, fleet_ids: [FLEET_B] });
  });

  it("keeps up through the idle grace, taking a hello nobody is listening for", () => {
    const leave = subscribeWorkspaceFrames(WS, () => {});
    connection(0).open();
    greet(connection(0), { fleet_ids: [FLEET_A] });
    leave();
    greet(connection(0), { fleet_ids: [FLEET_A, FLEET_B] });

    subscribeWorkspaceFrames(WS, () => {});
    expect(lastGreeting(WS)?.fleet_ids).toEqual([FLEET_A, FLEET_B]);
    expect(FakeEventSource.instances).toHaveLength(1);
    expect(connection(0).closed).toBe(false);
  });
});
