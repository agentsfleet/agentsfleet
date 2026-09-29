import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { BackfillOutcome } from "@/lib/streaming/fleet-stream-backfill";

import { FRAME_KIND } from "@/lib/api/events-types";
import { MAX_LIVE_EVENTS } from "@/lib/streaming/fleet-stream-cap";
import {
  BLANK_FEED,
  chunk,
  completed,
  CREATED_AT_MS,
  flushFrame,
  greet,
  listRow,
  noteServerFrameTime,
  push,
  received,
  reconnectBackfill,
  runWorkspaceBackfill,
  setupWorkspaceWire,
  toolStarted,
  TRIGGER_FEED,
  warnBackfillFailure,
  WORKSPACE_ID,
} from "@/tests/helpers/workspace-store-harness";
import { WorkspaceStore } from "./workspace-store";

const FLEET_A = "fleet_a";
const FLEET_B = "fleet_b";
const STRANGER = "fleet_stranger";
const STANDING = { events_processed: 3, budget_used_nanos: 900_000_000 };
const MOVED = { events_processed: 4, budget_used_nanos: 900_000_000 };
const OVERFLOW = MAX_LIVE_EVENTS + 50;
// The first row the cap keeps once OVERFLOW settled rows have landed.
const FIRST_KEPT = OVERFLOW - MAX_LIVE_EVENTS;
const MID_RUN_FRAMES = 10;
const TOOL = "search_repo";
const EVENT = "e1";
const UNSEEN = "e_unseen";

let store: WorkspaceStore;
let disconnect: () => void;

setupWorkspaceWire();

beforeEach(() => {
  store = new WorkspaceStore(WORKSPACE_ID);
  disconnect = store.connect([FLEET_A, FLEET_B]);
});

afterEach(() => disconnect());

describe("a tile hears only what it shows", () => {
  it("a chunk or tool call for a row the wall holds notifies no tile and keeps its snapshot", () => {
    const listener = vi.fn();
    store.subscribe(FLEET_A, listener);
    push(FLEET_A, received(FLEET_A, EVENT));
    flushFrame();
    expect(listener).toHaveBeenCalledTimes(1);
    const shown = store.snapshot(FLEET_A);
    for (let index = 0; index < MID_RUN_FRAMES; index += 1) {
      push(FLEET_A, chunk(FLEET_A, EVENT, `word ${index} `));
      push(FLEET_A, toolStarted(FLEET_A, EVENT, TOOL));
    }
    flushFrame();
    expect(listener).toHaveBeenCalledTimes(1);
    expect(store.snapshot(FLEET_A)).toBe(shown);
    expect(shown.feed).toBe(TRIGGER_FEED);
  });

  it("a chunk for a row the wall never saw opens it, so the feed reads as it always did", () => {
    push(FLEET_A, received(FLEET_A, EVENT));
    expect(store.snapshot(FLEET_A).feed).toBe(TRIGGER_FEED);
    push(FLEET_A, chunk(FLEET_A, UNSEEN));
    expect(store.snapshot(FLEET_A).feed).toBe(BLANK_FEED);
  });

  it("a tool call for a row the wall never saw opens nothing", () => {
    push(FLEET_A, received(FLEET_A, EVENT));
    const shown = store.snapshot(FLEET_A);
    push(FLEET_A, toolStarted(FLEET_A, UNSEEN, TOOL));
    expect(store.snapshot(FLEET_A)).toBe(shown);
  });

  it("a completion that moves neither the feed nor the counters keeps the snapshot; one that moves them does not", () => {
    push(FLEET_A, received(FLEET_A, EVENT, STANDING));
    const shown = store.snapshot(FLEET_A);
    push(FLEET_A, completed(FLEET_A, EVENT, STANDING));
    expect(store.snapshot(FLEET_A)).toBe(shown);
    push(FLEET_A, completed(FLEET_A, EVENT, MOVED));
    expect(store.snapshot(FLEET_A)).not.toBe(shown);
    expect(store.snapshot(FLEET_A).counters?.eventsProcessed).toBe(MOVED.events_processed);
  });

  it("a greeting still reaches every tile", () => {
    push(FLEET_A, received(FLEET_A, EVENT));
    const shown = store.snapshot(FLEET_A);
    greet({ kind: FRAME_KIND.HELLO, fleet_ids: [FLEET_A] });
    expect(store.snapshot(FLEET_A)).not.toBe(shown);
    expect(store.snapshot(FLEET_A).helloReceived).toBe(true);
  });
});

describe("the event map stays bounded", () => {
  it("the_store_still_bounds_its_event_map_without_absorb", () => {
    // `absorb` was the only thing that ever emptied a fleet's rows; with it
    // gone the cap has to hold on the write itself. The window shows through
    // the feed: a late chunk for a row the cap shed finds nothing and opens a
    // blank row, while one for a row still held changes nothing.
    for (let index = 0; index < OVERFLOW; index += 1) {
      push(FLEET_A, received(FLEET_A, `e${index}`));
      push(FLEET_A, completed(FLEET_A, `e${index}`));
    }
    push(FLEET_A, chunk(FLEET_A, `e${FIRST_KEPT}`));
    expect(store.snapshot(FLEET_A).feed).toBe(TRIGGER_FEED);
    push(FLEET_A, chunk(FLEET_A, `e${FIRST_KEPT - 1}`));
    expect(store.snapshot(FLEET_A).feed).toBe(BLANK_FEED);
    // And a key only for a fleet the wall subscribed: the map cannot grow
    // with the workspace, only with the tiles on screen.
    expect(store.snapshot(FLEET_B).feed).toBeUndefined();
  });

  it("a backfill that rejects is logged and leaves the fleet as it was", async () => {
    runWorkspaceBackfill.mockRejectedValue(new Error("network failed"));
    push(FLEET_A, received(FLEET_A, EVENT, STANDING));
    reconnectBackfill();
    await vi.waitFor(() => expect(warnBackfillFailure).toHaveBeenCalledTimes(1));
    expect(store.snapshot(FLEET_A).feed).toBe(TRIGGER_FEED);
    expect(store.snapshot(FLEET_A).counters?.eventsProcessed).toBe(STANDING.events_processed);
  });

  it("a backfill that lands after the store reconnected is dropped, catching up left standing", async () => {
    let release: (outcome: BackfillOutcome) => void = () => {};
    runWorkspaceBackfill.mockImplementation(
      () => new Promise<BackfillOutcome>((resolve) => { release = resolve; }),
    );
    greet({ kind: FRAME_KIND.CATCHING_UP, dropped: 2 });
    reconnectBackfill();
    await vi.waitFor(() => expect(runWorkspaceBackfill).toHaveBeenCalledTimes(1));
    // A new generation: the answer belongs to a connection this store no
    // longer represents.
    disconnect();
    disconnect = store.connect([FLEET_A]);
    release({ ok: true, watermark: 5 });
    await Promise.resolve();
    expect(store.snapshot(FLEET_A).catchingUp).toBe(true);
    expect(noteServerFrameTime).not.toHaveBeenCalled();
  });

  it("a walk that ends while a later gap's walk is queued leaves catching up to that walk", async () => {
    const releases: Array<(outcome: BackfillOutcome) => void> = [];
    runWorkspaceBackfill.mockImplementation(
      () => new Promise<BackfillOutcome>((resolve) => { releases.push(resolve); }),
    );
    // The transport runs the first gap's walk at once and queues the second's.
    greet({ kind: FRAME_KIND.CATCHING_UP, dropped: 2 });
    reconnectBackfill();
    greet({ kind: FRAME_KIND.CATCHING_UP, dropped: 3 });
    await vi.waitFor(() => expect(runWorkspaceBackfill).toHaveBeenCalledTimes(1));

    releases[0]?.({ ok: true, watermark: 5 });
    await vi.waitFor(() => expect(noteServerFrameTime).toHaveBeenCalledTimes(1));
    expect(store.snapshot(FLEET_A).catchingUp).toBe(true);

    reconnectBackfill();
    await vi.waitFor(() => expect(runWorkspaceBackfill).toHaveBeenCalledTimes(2));
    releases[1]?.({ ok: true, watermark: 6 });
    await vi.waitFor(() => expect(store.snapshot(FLEET_A).catchingUp).toBe(false));
  });

  it("a backfill row for a fleet the wall never subscribed is dropped", async () => {
    runWorkspaceBackfill.mockImplementation(async (req) => {
      req.onPage([listRow(STRANGER, "b1")]);
      return { ok: true, watermark: CREATED_AT_MS };
    });
    reconnectBackfill();
    await vi.waitFor(() => expect(noteServerFrameTime).toHaveBeenCalledWith(WORKSPACE_ID, CREATED_AT_MS));
    expect(store.snapshot(STRANGER).feed).toBeUndefined();
  });

  it("a reconnect backfill is capped on the same write", async () => {
    runWorkspaceBackfill.mockImplementation(async (req) => {
      req.onPage(Array.from({ length: OVERFLOW }, (_, index) => listRow(FLEET_A, `b${index}`, CREATED_AT_MS + index)));
      return { ok: true, watermark: null };
    });
    reconnectBackfill();
    await vi.waitFor(() => expect(runWorkspaceBackfill).toHaveBeenCalledTimes(1));
    push(FLEET_A, chunk(FLEET_A, `b${FIRST_KEPT}`));
    expect(store.snapshot(FLEET_A).feed).toBe(TRIGGER_FEED);
    push(FLEET_A, chunk(FLEET_A, `b${FIRST_KEPT - 1}`));
    expect(store.snapshot(FLEET_A).feed).toBe(BLANK_FEED);
  });
});
