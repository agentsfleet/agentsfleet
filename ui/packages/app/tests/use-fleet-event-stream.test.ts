import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { CONNECTION_STATUS, useFleetEventStream } from "../components/domain/useFleetEventStream";
import { __resetRegistryForTests } from "@/lib/streaming/fleet-stream-registry";
import { OUTCOME } from "@/lib/events/event-summary";
import { FRAME_KIND } from "@/lib/api/events-types";
import { FakeEventSource } from "./helpers/fake-event-source";
import { row, mount, WS, ZID } from "./helpers/fleet-stream-hook-fixtures";

describe("useFleetEventStream", () => {
  beforeEach(() => {
    FakeEventSource.install();
    __resetRegistryForTests();
  });

  afterEach(() => {
    cleanup();
    __resetRegistryForTests();
    FakeEventSource.uninstall();
  });

  it("opens an EventSource against the same-origin stream URL on mount", () => {
    mount();
    expect(FakeEventSource.instances.length).toBe(1);
    expect(FakeEventSource.instances[0]!.url).toBe(
      "/live/v1/workspaces/ws_1/fleets/zomb_1/events/stream",
    );
  });

  it("the stream URL carries no client token — cookie-authed, no bearer", () => {
    mount([row()]);
    const url = FakeEventSource.instances[0]!.url;
    expect(url).not.toMatch(/token/i);
    expect(url).not.toMatch(/authorization/i);
  });

  it("starts in CONNECTING and flips to LIVE only after a heartbeat", async () => {
    const { result } = mount();
    expect(result.current.connectionStatus).toBe(CONNECTION_STATUS.CONNECTING);
    act(() => FakeEventSource.instances[0]!.open());
    expect(result.current.connectionStatus).toBe(CONNECTION_STATUS.CONNECTING);
    act(() => FakeEventSource.instances[0]!.heartbeat());
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

  it("reconciles a refreshed terminal row without reopening the live stream", async () => {
    const received = row({ status: "received", response_text: null });
    const view = renderHook(
      ({ initial }) => useFleetEventStream(WS, ZID, initial),
      { initialProps: { initial: [received] } },
    );
    await waitFor(() => expect(view.result.current.events).toHaveLength(1));
    const source = FakeEventSource.instances[0];

    view.rerender({
      initial: [row({
        status: "fleet_error",
        response_text: null,
        failure_label: "startup_posture",
      })],
    });

    await waitFor(() => expect(view.result.current.events[0]).toMatchObject({
      status: "fleet_error",
      outcome: "Failed a startup safety check",
    }));
    expect(FakeEventSource.instances).toHaveLength(1);
    expect(FakeEventSource.instances[0]).toBe(source);
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
    await waitFor(() => expect(result.current.events[0]?.reply).toBe("Hello, world."));
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
    // Work is reported on the event itself, not as a thread-wide flag: an
    // aggregate "something is running" was true forever once any run stranded.
    expect(result.current.events[0]!.status).toBe("received");
    expect(result.current.events[0]!.outcome).toBe(OUTCOME.WORKING);
    act(() => {
      FakeEventSource.instances[0]!.emit({
        kind: FRAME_KIND.EVENT_COMPLETE,
        event_id: "evt_done",
        status: "processed",
      });
    });
    await waitFor(() => expect(result.current.events[0]!.status).toBe("processed"));
    // The outcome follows the status — a finished event stops saying it works.
    expect(result.current.events[0]!.outcome).toBe(OUTCOME.COMPLETED);
  });

  it("a stranded event cannot report the whole fleet as working", () => {
    // The hook exposes no aggregate running flag at all. That flag was read by
    // the composer's old hold, so one run that never completed silently turned
    // the console read-only.
    const { result } = mount();
    expect("isRunning" in result.current).toBe(false);
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
    expect(result.current.convertEvent(result.current.events[0]!).metadata?.custom?.["queued"]).toBe(true);

    act(() => {
      result.current.reconcileOptimistic(tempId, "evt_real");
    });
    await waitFor(() => expect(result.current.events[0]!.id).toBe("evt_real"));
    expect(result.current.events[0]!.status).toBe("received");
    expect(result.current.convertEvent(result.current.events[0]!).metadata?.custom?.["queued"]).toBe(true);
    act(() => FakeEventSource.instances[0]!.emit({
      kind: FRAME_KIND.EVENT_RECEIVED,
      event_id: "evt_real",
      actor: "steer:alice@example.com",
      created_at: Date.now(),
    }));
    expect(result.current.convertEvent(result.current.events[0]!).metadata?.custom?.["queued"]).toBe(false);
  });

  it("keeps a later steer queued while the first run streams, then starts it without mixing replies", async () => {
    const { result } = mount();
    let first = "";
    let second = "";
    act(() => {
      first = result.current.appendOptimistic("remember me", "steer:alice@example.com");
      second = result.current.appendOptimistic("what did I say?", "steer:alice@example.com");
      result.current.reconcileOptimistic(first, "evt_first");
      result.current.reconcileOptimistic(second, "evt_second");
    });
    const source = FakeEventSource.instances[0]!;
    act(() => {
      source.emit({ kind: FRAME_KIND.EVENT_RECEIVED, event_id: "evt_first", actor: "steer:alice@example.com", created_at: 1 });
      source.emit({ kind: FRAME_KIND.CHUNK, event_id: "evt_first", text: "Saved." });
    });
    await waitFor(() => expect(result.current.events.map((event) => event.reply)).toEqual(["Saved.", ""]));
    expect(result.current.convertEvent(result.current.events[1]!).metadata?.custom?.["queued"]).toBe(true);

    act(() => {
      source.emit({ kind: FRAME_KIND.EVENT_COMPLETE, event_id: "evt_first", status: "processed" });
      source.emit({ kind: FRAME_KIND.EVENT_RECEIVED, event_id: "evt_second", actor: "steer:alice@example.com", created_at: 2 });
      source.emit({ kind: FRAME_KIND.CHUNK, event_id: "evt_second", text: "You said remember me." });
    });
    await waitFor(() => expect(result.current.events.map((event) => event.reply)).toEqual(["Saved.", "You said remember me."]));
    expect(result.current.convertEvent(result.current.events[1]!).metadata?.custom?.["queued"]).toBe(false);
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

  it("discardOptimistic removes the failed row so a retry cannot duplicate it", async () => {
    const { result } = mount();
    let tempId = "";
    act(() => {
      tempId = result.current.appendOptimistic("send that fails", "steer:pending");
    });
    await waitFor(() => expect(result.current.events).toHaveLength(1));
    act(() => result.current.markOptimisticFailed(tempId));
    await waitFor(() => expect(result.current.events[0]!.status).toBe("failed"));
    act(() => result.current.discardOptimistic(tempId));
    await waitFor(() => expect(result.current.events).toHaveLength(0));
  });

  it("flips to RECONNECTING on onerror and reopens via backoff", async () => {
    vi.useFakeTimers();
    const { result } = mount();
    act(() => FakeEventSource.instances[0]!.open());
    expect(result.current.connectionStatus).toBe(CONNECTION_STATUS.CONNECTING);
    act(() => FakeEventSource.instances[0]!.heartbeat());
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

});
