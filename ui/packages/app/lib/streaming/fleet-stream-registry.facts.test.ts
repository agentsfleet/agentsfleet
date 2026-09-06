import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { type EventDetail, type LiveFrame } from "@/lib/api/events";
import { FRAME_KIND } from "@/lib/api/events-types";
import { NO_FACTS } from "@/lib/events/run-summary";
import { FakeEventSource } from "@/tests/helpers/fake-event-source";
import {
  __resetRegistryForTests,
  appendOptimistic,
  getSnapshot,
  reconcileServerFacts,
  reconcileServerRows,
  subscribe,
} from "./fleet-stream-registry";

// The registry as the strip's store: the fleet facts the tail carries, and
// the newest row's figures. Split from the registry suite by concern.

const WS = "ws_1";
const FLEET = "fleet_a";
const SEED_AT = Date.UTC(2026, 4, 15, 18, 30, 0);

function row(over: Partial<EventDetail> = {}): EventDetail {
  return {
    event_id: "evt_seed",
    fleet_id: FLEET,
    workspace_id: WS,
    actor: "cron:*",
    event_type: "cron",
    status: "processed",
    request_json: "{}",
    response_text: "seed body",
    tokens: 1500,
    wall_ms: 10,
    cost_nanos: null,
    failure_label: null,
    failure_detail: null,
    checkpoint_id: null,
    resumes_event_id: null,
    created_at: SEED_AT,
    updated_at: SEED_AT,
    ...over,
  };
}

function completion(over: Partial<Extract<LiveFrame, { kind: "event_complete" }>> = {}): LiveFrame {
  return {
    kind: FRAME_KIND.EVENT_COMPLETE,
    ...row({ event_id: "evt_live", tokens: 1200, created_at: SEED_AT + 60_000 }),
    fleet_status: "active",
    pending_approvals: 0,
    ...over,
  };
}

function open(initial: EventDetail[] = [row()]) {
  const release = subscribe(WS, FLEET, initial, () => {});
  const es = FakeEventSource.instances[0]!;
  return { release, emit: (frame: LiveFrame) => es.emit(frame) };
}

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

describe("fleet-stream-registry — fleet facts", () => {
  it("seeds with no facts and the seed's newest figures", () => {
    const { release } = open();
    expect(getSnapshot(FLEET).fleet).toBe(NO_FACTS);
    expect(getSnapshot(FLEET).latest).toMatchObject({ tokens: 1500, created_at: SEED_AT });
    release();
  });

  it("a completion folds both facts and the row's figures in", () => {
    const { release, emit } = open();
    emit({ kind: FRAME_KIND.EVENT_RECEIVED, event_id: "evt_live", actor: "cron:*" });
    emit(completion({ fleet_status: "paused", pending_approvals: 2 }));
    const snap = getSnapshot(FLEET);
    expect(snap.fleet).toEqual({ status: "paused", pendingApprovals: 2 });
    expect(snap.latest).toMatchObject({ tokens: 1200, status: "processed" });
    release();
  });

  it("a gate frame moves the count and no row", () => {
    const { release, emit } = open();
    const before = getSnapshot(FLEET);
    emit({ kind: FRAME_KIND.GATE_OPENED, gate_id: "g", event_id: "evt_seed", pending_approvals: 1 });
    const after = getSnapshot(FLEET);
    expect(after.fleet).toEqual({ status: null, pendingApprovals: 1 });
    expect(after.events).toBe(before.events);
    expect(after.latest).toBe(before.latest);
    release();
  });

  it("a server render's facts overwrite the tail's, and the next frame overwrites them back", () => {
    const { release, emit } = open();
    emit(completion({ fleet_status: "paused", pending_approvals: 1 }));
    reconcileServerFacts(FLEET, { status: "killed", pendingApprovals: 0 });
    expect(getSnapshot(FLEET).fleet).toEqual({ status: "killed", pendingApprovals: 0 });
    emit({ kind: FRAME_KIND.GATE_OPENED, gate_id: "g", event_id: "e", pending_approvals: 1 });
    expect(getSnapshot(FLEET).fleet).toEqual({ status: "killed", pendingApprovals: 1 });
    release();
  });

  it("a server render for a fleet nobody is watching is dropped, not stored", () => {
    reconcileServerFacts("fleet_nobody_watches", { status: "active", pendingApprovals: 1 });
    expect(getSnapshot("fleet_nobody_watches").fleet).toBe(NO_FACTS);
  });

  it("a completion wakes subscribers once, with its row and its facts in one snapshot", () => {
    const listener = vi.fn();
    const release = subscribe(WS, FLEET, [row()], listener);
    const es = FakeEventSource.instances[0]!;
    es.emit({ kind: FRAME_KIND.EVENT_RECEIVED, event_id: "evt_live", actor: "cron:*" });
    const woken = listener.mock.calls.length;
    es.emit(completion({ fleet_status: "paused", pending_approvals: 3 }));
    expect(listener.mock.calls.length).toBe(woken + 1);
    const snap = getSnapshot(FLEET);
    expect(snap.fleet).toEqual({ status: "paused", pendingApprovals: 3 });
    expect(snap.latest).toMatchObject({ tokens: 1200 });
    release();
  });

  it("only frames advance the facts sequence; a server render never does", () => {
    const { release, emit } = open();
    const before = getSnapshot(FLEET).factsSeq;
    emit({ kind: FRAME_KIND.GATE_OPENED, gate_id: "g", event_id: "e", pending_approvals: 1 });
    expect(getSnapshot(FLEET).factsSeq).toBe(before + 1);
    // A frame that restates the facts is not a word spoken.
    emit({ kind: FRAME_KIND.GATE_OPENED, gate_id: "g2", event_id: "e", pending_approvals: 1 });
    expect(getSnapshot(FLEET).factsSeq).toBe(before + 1);
    reconcileServerFacts(FLEET, { status: "killed", pendingApprovals: 0 });
    const landed = getSnapshot(FLEET).fleet;
    expect(landed).toEqual({ status: "killed", pendingApprovals: 0 });
    expect(getSnapshot(FLEET).factsSeq).toBe(before + 1);
    // A render restating what the snapshot already holds changes nothing —
    // the facts keep their identity, so no subscriber wakes for it.
    reconcileServerFacts(FLEET, { status: "killed", pendingApprovals: 0 });
    expect(getSnapshot(FLEET).fleet).toBe(landed);
    release();
  });

  it("a frame that restates the facts does not wake subscribers", () => {
    const listener = vi.fn();
    const release = subscribe(WS, FLEET, [row()], listener);
    const es = FakeEventSource.instances[0]!;
    es.emit({ kind: FRAME_KIND.GATE_OPENED, gate_id: "g", event_id: "e", pending_approvals: 1 });
    const woken = listener.mock.calls.length;
    es.emit({ kind: FRAME_KIND.GATE_RESOLVED, gate_id: "g", event_id: "e", status: "approved", resolved_by: "x", pending_approvals: 1 });
    expect(listener.mock.calls.length).toBe(woken);
    release();
  });
});

describe("fleet-stream-registry — the newest figures", () => {
  it("keep their identity across chunks of a streaming reply", () => {
    const { release, emit } = open();
    emit({ kind: FRAME_KIND.EVENT_RECEIVED, event_id: "evt_live", actor: "fleet", created_at: SEED_AT + 1 });
    const opened = getSnapshot(FLEET).latest;
    expect(opened).toMatchObject({ status: "received", tokens: null });
    emit({ kind: FRAME_KIND.CHUNK, event_id: "evt_live", text: "Hel" });
    emit({ kind: FRAME_KIND.CHUNK, event_id: "evt_live", text: "lo" });
    expect(getSnapshot(FLEET).latest).toBe(opened);
    release();
  });

  it("move when a backfill lands a newer terminal row", () => {
    const { release } = open();
    reconcileServerRows(FLEET, [row({ event_id: "evt_missed", tokens: 7, created_at: SEED_AT + 5 })]);
    expect(getSnapshot(FLEET).latest).toMatchObject({ tokens: 7 });
    release();
  });

  it("never point at the operator's own message before the server names it", () => {
    const { release } = open();
    appendOptimistic(FLEET, "deploy it", "steer:me");
    expect(getSnapshot(FLEET).latest).toMatchObject({ tokens: 1500 });
    release();
  });
});
