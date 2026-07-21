import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";

import {
  CONNECTION_STATUS,
  useFleetEventStream,
} from "../components/domain/useFleetEventStream";
import { __resetRegistryForTests } from "@/lib/streaming/fleet-stream-registry";
import { OUTCOME } from "@/lib/events/event-summary";
import { FRAME_KIND, type EventRow, type LiveFrame } from "@/lib/api/events";

// ── FakeEventSource ────────────────────────────────────────────────────────
// Standalone SSE-test harness. Future SSE-touching tests should
// inline the same pattern; centralizing was considered and rejected
// (the helper is small enough that the duplication is cheaper than
// the extra import surface).

type EsHandlers = {
  onopen: ((this: EventSource, ev: Event) => unknown) | null;
  onmessage: ((this: EventSource, ev: MessageEvent) => unknown) | null;
  onerror: ((this: EventSource, ev: Event) => unknown) | null;
};

class FakeEventSource implements EsHandlers {
  static instances: FakeEventSource[] = [];
  url: string;
  onopen: EsHandlers["onopen"] = null;
  onmessage: EsHandlers["onmessage"] = null;
  onerror: EsHandlers["onerror"] = null;
  closed = false;
  constructor(url: string) {
    this.url = url;
    FakeEventSource.instances.push(this);
  }
  close() {
    this.closed = true;
  }
  emit(frame: LiveFrame) {
    this.onmessage?.call(this as unknown as EventSource, {
      data: JSON.stringify(frame),
    } as MessageEvent);
  }
  open() {
    this.onopen?.call(this as unknown as EventSource, {} as Event);
  }
  fail() {
    this.onerror?.call(this as unknown as EventSource, {} as Event);
  }
}

// ── Fixtures ───────────────────────────────────────────────────────────────

function row(over: Partial<EventRow> = {}): EventRow {
  const now = Date.UTC(2026, 4, 15, 18, 30, 0);
  return {
    event_id: "evt_seed",
    fleet_id: "zomb_1",
    workspace_id: "ws_1",
    actor: "alice@example.com",
    event_type: "chat",
    status: "processed",
    request_json: "{}",
    response_text: "seed body",
    tokens: 1,
    wall_ms: 10,
    cost_nanos: null,
    failure_label: null,
    checkpoint_id: null,
    resumes_event_id: null,
    created_at: now,
    updated_at: now,
    ...over,
  };
}

const WS = "ws_1";
const ZID = "zomb_1";

function mount(initial: EventRow[] = []) {
  return renderHook(() => useFleetEventStream(WS, ZID, initial));
}

describe("useFleetEventStream", () => {
  beforeEach(() => {
    FakeEventSource.instances = [];
    (globalThis as unknown as { EventSource: unknown }).EventSource = FakeEventSource;
    __resetRegistryForTests();
  });

  afterEach(() => {
    cleanup();
    __resetRegistryForTests();
    delete (globalThis as { EventSource?: unknown }).EventSource;
  });

  it("opens an EventSource against the same-origin stream URL on mount", () => {
    mount();
    expect(FakeEventSource.instances.length).toBe(1);
    expect(FakeEventSource.instances[0]!.url).toBe(
      "/backend/v1/workspaces/ws_1/fleets/zomb_1/events/stream",
    );
  });

  it("the stream URL carries no client token — cookie-authed, no bearer", () => {
    mount([row()]);
    const url = FakeEventSource.instances[0]!.url;
    expect(url).not.toMatch(/token/i);
    expect(url).not.toMatch(/authorization/i);
  });

  it("starts in CONNECTING and flips to LIVE on onopen", async () => {
    const { result } = mount();
    expect(result.current.connectionStatus).toBe(CONNECTION_STATUS.CONNECTING);
    act(() => FakeEventSource.instances[0]!.open());
    await waitFor(() => {
      expect(result.current.connectionStatus).toBe(CONNECTION_STATUS.LIVE);
    });
  });

  it("lets the operator retry the connection immediately", () => {
    const { result } = mount();
    const first = FakeEventSource.instances[0]!;
    act(() => result.current.retryConnection());
    expect(first.closed).toBe(true);
    expect(result.current.connectionStatus).toBe(CONNECTION_STATUS.CONNECTING);
    expect(FakeEventSource.instances).toHaveLength(2);
  });

  it("deduplicates a double error into one reconnect", async () => {
    vi.useFakeTimers();
    try {
      mount();
      expect(FakeEventSource.instances.length).toBe(1);
      // Repeated error callbacks from one source share the already-scheduled
      // reconnect, so only one replacement EventSource is created.
      act(() => FakeEventSource.instances[0]!.fail());
      act(() => FakeEventSource.instances[0]!.fail());
      await act(async () => {
        await vi.advanceTimersByTimeAsync(20_000);
      });
      expect(FakeEventSource.instances.length).toBe(2);
    } finally {
      vi.useRealTimers();
    }
  });

  it("seeds from the server-rendered initial rows and sorts by createdAt ascending", async () => {
    const t0 = Date.UTC(2026, 4, 15, 18, 0, 0);
    const t1 = Date.UTC(2026, 4, 15, 18, 30, 0);
    const { result } = mount([
      row({ event_id: "evt_newer", created_at: t1, response_text: "second" }),
      row({ event_id: "evt_older", created_at: t0, response_text: "first" }),
    ]);
    await waitFor(() => expect(result.current.events).toHaveLength(2));
    expect(result.current.events.map((e) => e.id)).toEqual(["evt_older", "evt_newer"]);
  });

  it("maps actor → role: steer:* → user, webhook:* → system, fleet → assistant", async () => {
    const { result } = mount([
      row({ event_id: "u", actor: "steer:alice@example.com" }),
      row({ event_id: "w", actor: "webhook:github" }),
      row({ event_id: "a", actor: "fleet" }),
      row({ event_id: "c", actor: "cron" }),
    ]);
    await waitFor(() => expect(result.current.events).toHaveLength(4));
    const byId = new Map(result.current.events.map((e) => [e.id, e]));
    expect(byId.get("u")!.role).toBe("user");
    expect(byId.get("w")!.role).toBe("system");
    expect(byId.get("a")!.role).toBe("assistant");
    expect(byId.get("c")!.role).toBe("system");
  });

  it("appends new live-stream EVENT_RECEIVED frames after the seed", async () => {
    const { result } = mount([row({ event_id: "evt_seed" })]);
    await waitFor(() => expect(result.current.events).toHaveLength(1));
    act(() => {
      FakeEventSource.instances[0]!.emit({
        kind: FRAME_KIND.EVENT_RECEIVED,
        event_id: "evt_live",
        actor: "webhook:github",
      });
    });
    await waitFor(() => expect(result.current.events).toHaveLength(2));
    expect(result.current.events[1]!.id).toBe("evt_live");
    expect(result.current.events[1]!.role).toBe("system");
    expect(result.current.events[1]!.status).toBe("received");
  });

  it("CHUNK frames concatenate text on the assistant message for that event_id", async () => {
    const { result } = mount();
    act(() => {
      FakeEventSource.instances[0]!.emit({
        kind: FRAME_KIND.EVENT_RECEIVED,
        event_id: "evt_run",
        actor: "fleet",
      });
    });
    act(() => {
      FakeEventSource.instances[0]!.emit({
        kind: FRAME_KIND.CHUNK,
        event_id: "evt_run",
        text: "Hello, ",
      });
    });
    act(() => {
      FakeEventSource.instances[0]!.emit({
        kind: FRAME_KIND.CHUNK,
        event_id: "evt_run",
        text: "world.",
      });
    });
    await waitFor(() => expect(result.current.events).toHaveLength(1));
    expect(result.current.events[0]!.text).toBe("Hello, world.");
    expect(result.current.events[0]!.role).toBe("assistant");
  });

  it("EVENT_COMPLETE updates the event status to processed", async () => {
    const { result } = mount();
    act(() => {
      FakeEventSource.instances[0]!.emit({
        kind: FRAME_KIND.EVENT_RECEIVED,
        event_id: "evt_done",
        actor: "fleet",
      });
    });
    await waitFor(() => expect(result.current.events).toHaveLength(1));
    expect(result.current.isRunning).toBe(true);
    act(() => {
      FakeEventSource.instances[0]!.emit({
        kind: FRAME_KIND.EVENT_COMPLETE,
        event_id: "evt_done",
        status: "processed",
      });
    });
    await waitFor(() => expect(result.current.events[0]!.status).toBe("processed"));
    expect(result.current.isRunning).toBe(false);
  });

  it("appendOptimistic + reconcileOptimistic swaps the temp id for the real one", async () => {
    const { result } = mount();
    let tempId = "";
    act(() => {
      tempId = result.current.appendOptimistic("howdy", "steer:alice@example.com");
    });
    await waitFor(() => expect(result.current.events).toHaveLength(1));
    expect(result.current.events[0]!.id).toBe(tempId);
    expect(result.current.events[0]!.status).toBe("optimistic");
    expect(result.current.events[0]!.text).toBe("howdy");
    expect(result.current.events[0]!.role).toBe("user");

    act(() => {
      result.current.reconcileOptimistic(tempId, "evt_real");
    });
    await waitFor(() => expect(result.current.events[0]!.id).toBe("evt_real"));
    expect(result.current.events[0]!.status).toBe("received");
  });

  it("markOptimisticFailed flips the optimistic message to failed", async () => {
    const { result } = mount();
    let tempId = "";
    act(() => {
      tempId = result.current.appendOptimistic("send that fails", "steer:pending");
    });
    await waitFor(() => expect(result.current.events).toHaveLength(1));
    act(() => result.current.markOptimisticFailed(tempId));
    await waitFor(() => expect(result.current.events[0]!.status).toBe("failed"));
    expect(result.current.events[0]!.id).toBe(tempId);
  });

  it("flips to RECONNECTING on onerror and reopens via backoff", async () => {
    vi.useFakeTimers();
    const { result } = mount();
    act(() => FakeEventSource.instances[0]!.open());
    await vi.waitFor(() => {
      expect(result.current.connectionStatus).toBe(CONNECTION_STATUS.LIVE);
    });
    act(() => FakeEventSource.instances[0]!.fail());
    expect(result.current.connectionStatus).toBe(CONNECTION_STATUS.RECONNECTING);
    expect(FakeEventSource.instances[0]!.closed).toBe(true);
    expect(FakeEventSource.instances).toHaveLength(1);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(2_000);
    });
    expect(FakeEventSource.instances).toHaveLength(2);
    vi.useRealTimers();
  });

  it("convertEvent produces an assistant-ui ThreadMessageLike with custom metadata", () => {
    const { result } = mount();
    const msg = result.current.convertEvent({
      id: "evt_x",
      role: "system",
      actor: "webhook:github",
      text: "workflow_run failure",
      outcome: OUTCOME.NO_REPLY,
      createdAt: new Date(0),
      status: "processed",
      custom: { requestJson: '{"action":"workflow_run"}' },
    });
    expect(msg.role).toBe("system");
    expect(msg.id).toBe("evt_x");
    expect(msg.content).toEqual([{ type: "text", text: "workflow_run failure" }]);
    expect(msg.metadata?.custom?.actor).toBe("webhook:github");
    expect(msg.metadata?.custom?.requestJson).toBe('{"action":"workflow_run"}');
  });

  it("ignores SSE frames with malformed JSON", () => {
    const { result } = mount();
    act(() => {
      FakeEventSource.instances[0]!.onmessage?.call(
        FakeEventSource.instances[0]! as unknown as EventSource,
        { data: "this is not json" } as MessageEvent,
      );
    });
    expect(result.current.events).toEqual([]);
  });

  // ── Robustness: invalid/error paths ─────────────────────────────────────

  it("creates a fresh event row when CHUNK arrives before EVENT_RECEIVED", async () => {
    const { result } = mount();
    act(() => {
      FakeEventSource.instances[0]!.open();
    });
    await waitFor(() =>
      expect(result.current.connectionStatus).toBe(CONNECTION_STATUS.LIVE),
    );
    act(() => {
      FakeEventSource.instances[0]!.emit({
        kind: FRAME_KIND.CHUNK,
        event_id: "evt_orphan",
        text: "partial body without a header frame",
      } as LiveFrame);
    });
    await waitFor(() => {
      expect(result.current.events.length).toBe(1);
      expect(result.current.events[0]!.id).toBe("evt_orphan");
      expect(result.current.events[0]!.text).toBe(
        "partial body without a header frame",
      );
    });
  });

  it("drops SSE frames that parse to non-object values", async () => {
    const { result } = mount();
    const inputs: unknown[] = [null, 42, '"a string"', "[1,2,3]"];
    act(() => {
      for (const raw of inputs) {
        FakeEventSource.instances[0]!.onmessage?.call(
          FakeEventSource.instances[0]! as unknown as EventSource,
          {
            data: typeof raw === "string" ? raw : JSON.stringify(raw),
          } as MessageEvent,
        );
      }
    });
    expect(result.current.events).toEqual([]);
  });

  it("drops SSE frames with unknown kind (default-switch arm)", async () => {
    const { result } = mount();
    act(() => {
      FakeEventSource.instances[0]!.onmessage?.call(
        FakeEventSource.instances[0]! as unknown as EventSource,
        {
          data: JSON.stringify({ kind: "future_kind_we_dont_know" }),
        } as MessageEvent,
      );
    });
    expect(result.current.events).toEqual([]);
  });

  // ── install:* frames advance the install step, off the chat path ─────────
  // The post-create install progression rides the SAME SSE stream this hook
  // owns; these prove the registry forks `install:*` frames into `installStep`
  // and advances it monotonically, and that `install:ready` is the flip signal.

  it("advances installStep through creating → provisioning → ready as install:* frames arrive", async () => {
    const { result } = mount();
    expect(result.current.installStep).toBeNull();

    act(() => FakeEventSource.instances[0]!.emit({ kind: FRAME_KIND.INSTALL_CREATING }));
    await waitFor(() => expect(result.current.installStep).toBe("creating"));

    act(() => FakeEventSource.instances[0]!.emit({ kind: FRAME_KIND.INSTALL_PROVISIONING }));
    await waitFor(() => expect(result.current.installStep).toBe("provisioning"));

    act(() => FakeEventSource.instances[0]!.emit({ kind: FRAME_KIND.INSTALL_READY }));
    await waitFor(() => expect(result.current.installStep).toBe("ready"));

    // Install frames never leak into the chat message list.
    expect(result.current.events).toEqual([]);
  });

  it("a late duplicate install frame never rewinds the rendered step", async () => {
    const { result } = mount();
    act(() => FakeEventSource.instances[0]!.emit({ kind: FRAME_KIND.INSTALL_PROVISIONING }));
    await waitFor(() => expect(result.current.installStep).toBe("provisioning"));
    // A stray re-emitted `creating` after `provisioning` must hold at provisioning.
    act(() => FakeEventSource.instances[0]!.emit({ kind: FRAME_KIND.INSTALL_CREATING }));
    expect(result.current.installStep).toBe("provisioning");
  });

  it("an install:error frame flips the step to error and chat stays empty", async () => {
    const { result } = mount();
    act(() => FakeEventSource.instances[0]!.emit({ kind: FRAME_KIND.INSTALL_CREATING }));
    await waitFor(() => expect(result.current.installStep).toBe("creating"));
    act(() => FakeEventSource.instances[0]!.emit({ kind: FRAME_KIND.INSTALL_ERROR }));
    await waitFor(() => expect(result.current.installStep).toBe("error"));
    expect(result.current.events).toEqual([]);
  });
});
