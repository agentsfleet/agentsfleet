import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";

const { listZombieEventsMock } = vi.hoisted(() => ({
  listZombieEventsMock: vi.fn(),
}));

vi.mock("@/lib/api/events", async () => {
  const actual = await vi.importActual<typeof import("@/lib/api/events")>(
    "@/lib/api/events",
  );
  return { ...actual, listZombieEvents: listZombieEventsMock };
});

import {
  CONNECTION_STATUS,
  useZombieEventStream,
} from "../components/domain/useZombieEventStream";
import { __resetRegistryForTests } from "@/lib/streaming/zombie-stream-registry";
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
    event_id: "evt_backfill",
    zombie_id: "zomb_1",
    workspace_id: "ws_1",
    actor: "alice@example.com",
    event_type: "chat",
    status: "processed",
    request_json: "{}",
    response_text: "backfill body",
    tokens: 1,
    wall_ms: 10,
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
const TOKEN = "tok_test";

function mount(token: string | null = TOKEN) {
  return renderHook(() => useZombieEventStream(WS, ZID, token));
}

describe("useZombieEventStream", () => {
  beforeEach(() => {
    FakeEventSource.instances = [];
    (globalThis as unknown as { EventSource: unknown }).EventSource = FakeEventSource;
    listZombieEventsMock.mockReset();
    listZombieEventsMock.mockResolvedValue({ items: [], next_cursor: null });
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
      "/backend/v1/workspaces/ws_1/zombies/zomb_1/events/stream",
    );
  });

  it("starts in CONNECTING and flips to LIVE on onopen", async () => {
    const { result } = mount();
    expect(result.current.connectionStatus).toBe(CONNECTION_STATUS.CONNECTING);
    act(() => FakeEventSource.instances[0]!.open());
    await waitFor(() => {
      expect(result.current.connectionStatus).toBe(CONNECTION_STATUS.LIVE);
    });
  });

  it("backfills via listZombieEvents and sorts by createdAt ascending", async () => {
    const t0 = Date.UTC(2026, 4, 15, 18, 0, 0);
    const t1 = Date.UTC(2026, 4, 15, 18, 30, 0);
    listZombieEventsMock.mockResolvedValue({
      items: [
        row({ event_id: "evt_newer", created_at: t1, response_text: "second" }),
        row({ event_id: "evt_older", created_at: t0, response_text: "first" }),
      ],
      next_cursor: null,
    });
    const { result } = mount();
    await waitFor(() => expect(result.current.events).toHaveLength(2));
    expect(result.current.events.map((e) => e.id)).toEqual(["evt_older", "evt_newer"]);
  });

  it("maps actor → role: steer:* → user, webhook:* → system, agent → assistant", async () => {
    listZombieEventsMock.mockResolvedValue({
      items: [
        row({ event_id: "u", actor: "steer:alice@example.com" }),
        row({ event_id: "w", actor: "webhook:github" }),
        row({ event_id: "a", actor: "agent" }),
        row({ event_id: "c", actor: "cron" }),
      ],
      next_cursor: null,
    });
    const { result } = mount();
    await waitFor(() => expect(result.current.events).toHaveLength(4));
    const byId = new Map(result.current.events.map((e) => [e.id, e]));
    expect(byId.get("u")!.role).toBe("user");
    expect(byId.get("w")!.role).toBe("system");
    expect(byId.get("a")!.role).toBe("assistant");
    expect(byId.get("c")!.role).toBe("system");
  });

  it("appends new live-stream EVENT_RECEIVED frames after backfill", async () => {
    listZombieEventsMock.mockResolvedValue({
      items: [row({ event_id: "evt_backfill" })],
      next_cursor: null,
    });
    const { result } = mount();
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
        actor: "agent",
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
        actor: "agent",
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

    act(() => result.current.reconcileOptimistic(tempId, "evt_real"));
    await waitFor(() => expect(result.current.events[0]!.id).toBe("evt_real"));
    expect(result.current.events[0]!.status).toBe("received");
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

  // ── Robustness: null/invalid/error paths ────────────────────────────────

  it("skips bearer-authed backfill when token is null (SSE is cookie-authed and still opens)", () => {
    const { result } = mount(null);
    // SSE endpoint authenticates via the Next.js middleware cookie session,
    // not the token prop — so the stream still opens. Backfill (bearer-only)
    // is skipped.
    expect(FakeEventSource.instances.length).toBe(1);
    expect(listZombieEventsMock).not.toHaveBeenCalled();
    expect(result.current.events).toEqual([]);
    expect(result.current.connectionStatus).toBe(CONNECTION_STATUS.CONNECTING);
  });

  it("survives a backfill HTTP failure: events stays [], stream still opens", async () => {
    listZombieEventsMock.mockRejectedValueOnce(new Error("HTTP 503"));
    const { result } = mount();
    // The stream still opens (the SSE path is independent of backfill).
    expect(FakeEventSource.instances.length).toBe(1);
    await waitFor(() => {
      expect(result.current.events).toEqual([]);
    });
    // No unhandled rejection escapes the hook's try/catch.
    expect(result.current.connectionStatus).toBe(CONNECTION_STATUS.CONNECTING);
  });

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
});
