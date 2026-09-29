import type { ProfilerOnRenderCallback } from "react";

import { act, render } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { WorkspaceStreamProvider } from "@/components/domain/useWorkspaceStream";
import { FRAME_KIND } from "@/lib/api/events-types";
import {
  activityFrame,
  CATCHING_UP_LABEL,
  completionFrame,
  CURRENT_LABEL,
  eventRow,
  FakeEventSource,
  FEED_SHOWN_LABEL,
  FleetProbe,
  flushAnimationFrame,
  LAST_KNOWN_LABEL,
  LIVE_LABEL,
  NO_FEED_LABEL,
  onlyEventSource,
  renderWall,
  settlePromises,
  setupWallTests,
  WORKSPACE_ID,
} from "./helpers/fleets-wall-harness";

const FLEET_A = "fleet_a";
const FLEET_B = "fleet_b";
const BURST_FLEET_COUNT = 60;
const RECONNECT_DELAY_MS = 2_000;
const BACKFILL_PAGE_LIMIT = 200;
const SNAPSHOT_KIND_LABEL = "kind:snapshot";
const LIVE_KIND_LABEL = "kind:live";
const COUNT_BEFORE_THE_DROP = 5;
const COUNT_AFTER_THE_DROP = 7;

setupWallTests();

describe("workspace fleet wall provider", () => {
  it("opens one workspace stream and routes a tagged frame only to its fleet", () => {
    const view = renderWall([FLEET_A, FLEET_B]);
    const source = onlyEventSource();

    source.open();
    source.emit({ kind: FRAME_KIND.HELLO, fleet_ids: [FLEET_A, FLEET_B] });
    flushAnimationFrame();
    source.emit(activityFrame(FLEET_A));
    flushAnimationFrame();

    expect(FakeEventSource.instances).toHaveLength(1);
    expect(source.url).toBe(`/live/v1/workspaces/${WORKSPACE_ID}/events/stream`);
    expect(view.getByTestId(FLEET_A).textContent).toContain(FEED_SHOWN_LABEL);
    expect(view.getByTestId(FLEET_B).textContent).toContain(NO_FEED_LABEL);
  });

  it("coalesces a 60-fleet burst into one animation callback and one React commit", () => {
    const fleetIds = Array.from({ length: BURST_FLEET_COUNT }, (_, index) => `fleet_${index}`);
    const onRender = vi.fn<ProfilerOnRenderCallback>();
    renderWall(fleetIds, onRender);
    const source = onlyEventSource();

    source.open();
    source.emit({ kind: FRAME_KIND.HELLO, fleet_ids: fleetIds });
    flushAnimationFrame();
    const commitsBeforeBurst = onRender.mock.calls.length;
    vi.mocked(requestAnimationFrame).mockClear();

    for (const fleetId of fleetIds) source.emit(activityFrame(fleetId));
    expect(requestAnimationFrame).toHaveBeenCalledTimes(1);
    flushAnimationFrame();
    expect(onRender.mock.calls).toHaveLength(commitsBeforeBurst + 1);
  });

  it("marks fleet liveness from the server hello frame", () => {
    const view = renderWall([FLEET_A, FLEET_B]);
    const source = onlyEventSource();

    source.emit({ kind: FRAME_KIND.HELLO, fleet_ids: [FLEET_A] });
    flushAnimationFrame();

    expect(view.getByTestId(FLEET_A).textContent).toContain(LIVE_LABEL);
    expect(view.getByTestId(FLEET_B).textContent).toContain(LAST_KNOWN_LABEL);
  });

  it("updates fleet liveness only when a later server hello announces the changed set", () => {
    const view = renderWall([FLEET_A, FLEET_B]);
    const source = onlyEventSource();

    source.emit({ kind: FRAME_KIND.HELLO, fleet_ids: [FLEET_A] });
    flushAnimationFrame();
    expect(view.getByTestId(FLEET_B).textContent).toContain(LAST_KNOWN_LABEL);

    source.emit({ kind: FRAME_KIND.HELLO, fleet_ids: [FLEET_A, FLEET_B] });
    flushAnimationFrame();
    expect(view.getByTestId(FLEET_B).textContent).toContain(LIVE_LABEL);
  });

  it("surfaces catching up until the server drop gap is backfilled", async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({ items: [], next_cursor: null }),
    });
    vi.stubGlobal("fetch", fetchMock);
    const view = renderWall([FLEET_A]);
    const source = onlyEventSource();

    source.emit({ kind: FRAME_KIND.HELLO, fleet_ids: [FLEET_A] });
    flushAnimationFrame();
    source.emit({ kind: FRAME_KIND.CATCHING_UP, dropped: 3 });
    source.emit({ kind: FRAME_KIND.CATCHING_UP, dropped: 4 });
    flushAnimationFrame();

    expect(view.getByTestId(FLEET_A).textContent).toContain(CATCHING_UP_LABEL);
    await act(async () => settlePromises());
    flushAnimationFrame();
    // The second gap arrived during the first walk, so one more walk follows.
    expect(fetchMock).toHaveBeenCalledTimes(2);
    expect(view.getByTestId(FLEET_A).textContent).toContain(CURRENT_LABEL);
  });

  it("test_wall_shows_a_gap: a lost subscription, sent as zero drops, shows catching up until its backfill lands", async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({ items: [], next_cursor: null }),
    });
    vi.stubGlobal("fetch", fetchMock);
    const view = renderWall([FLEET_A]);
    const source = onlyEventSource();

    source.emit({ kind: FRAME_KIND.HELLO, fleet_ids: [FLEET_A] });
    flushAnimationFrame();
    // The count of missed frames is unknowable after a lost subscription.
    source.emit({ kind: FRAME_KIND.CATCHING_UP, dropped: 0 });
    flushAnimationFrame();

    expect(view.getByTestId(FLEET_A).textContent).toContain(CATCHING_UP_LABEL);
    await act(async () => settlePromises());
    flushAnimationFrame();
    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(view.getByTestId(FLEET_A).textContent).toContain(CURRENT_LABEL);
  });

  it("keeps catching up visible when the server drop backfill fails", async () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue({ ok: false, status: 503 }));
    const view = renderWall([FLEET_A]);
    const source = onlyEventSource();

    source.emit({ kind: FRAME_KIND.CATCHING_UP, dropped: 4 });
    flushAnimationFrame();
    await act(async () => settlePromises());
    flushAnimationFrame();

    expect(view.getByTestId(FLEET_A).textContent).toContain(CATCHING_UP_LABEL);
    expect(warn).toHaveBeenCalledWith("fleet-stream backfill failed", "HTTP 503");
  });

  it("keeps catching up visible when the backfill request rejects", async () => {
    const failure = new Error("network failed");
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    vi.stubGlobal("fetch", vi.fn().mockRejectedValue(failure));
    const view = renderWall([FLEET_A]);
    const source = onlyEventSource();

    source.emit({ kind: FRAME_KIND.CATCHING_UP, dropped: 5 });
    flushAnimationFrame();
    await act(async () => settlePromises());

    expect(view.getByTestId(FLEET_A).textContent).toContain(CATCHING_UP_LABEL);
    expect(warn).toHaveBeenCalledWith("fleet-stream backfill failed", failure);
  });

  it("recovers both fleets through one workspace backfill after reconnect", async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({
        items: [
          eventRow(FLEET_A, "recovered_a"),
          eventRow(FLEET_B, "recovered_b"),
          eventRow("unsubscribed", "ignored"),
        ],
        next_cursor: null,
      }),
    });
    vi.stubGlobal("fetch", fetchMock);
    const view = renderWall([FLEET_A, FLEET_B]);
    const first = onlyEventSource();
    first.open();
    flushAnimationFrame();

    first.fail();
    await act(async () => vi.advanceTimersByTimeAsync(RECONNECT_DELAY_MS));
    const second = FakeEventSource.instances[1];
    if (!second) throw new Error("workspace stream did not reconnect");
    await act(async () => {
      second.open();
      await settlePromises();
    });
    flushAnimationFrame();

    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(fetchMock.mock.calls[0]?.[0]).toBe(
      `/live/v1/workspaces/${WORKSPACE_ID}/events?limit=${BACKFILL_PAGE_LIMIT}`,
    );
    expect(view.getByTestId(FLEET_A).textContent).toContain(FEED_SHOWN_LABEL);
    expect(view.getByTestId(FLEET_B).textContent).toContain(FEED_SHOWN_LABEL);
  });

  it("returns a stable empty state when a tile renders outside the provider", () => {
    const view = render(<FleetProbe fleetId={FLEET_A} />);

    expect(view.getByTestId(FLEET_A).textContent).toContain(NO_FEED_LABEL);
    expect(view.getByTestId(FLEET_A).textContent).toContain(CURRENT_LABEL);
    expect(FakeEventSource.instances).toHaveLength(0);
  });

  it("shares one cached fleet state across duplicate consumers", () => {
    const view = render(
      <WorkspaceStreamProvider workspaceId={WORKSPACE_ID} fleetIds={[FLEET_A]}>
        <FleetProbe fleetId={FLEET_A} />
        <FleetProbe fleetId={FLEET_A} />
      </WorkspaceStreamProvider>,
    );
    const source = onlyEventSource();

    source.emit(activityFrame(FLEET_A));
    flushAnimationFrame();

    expect(view.getAllByTestId(FLEET_A)).toHaveLength(2);
    expect(view.getAllByTestId(FLEET_A)[0]?.textContent).toContain(FEED_SHOWN_LABEL);
  });

  it("skips a removed tile listener when its queued update flushes", () => {
    const fleetIds = [FLEET_A];
    const view = render(
      <WorkspaceStreamProvider workspaceId={WORKSPACE_ID} fleetIds={fleetIds}>
        <FleetProbe fleetId={FLEET_A} />
      </WorkspaceStreamProvider>,
    );
    const source = onlyEventSource();
    source.emit(activityFrame(FLEET_A));

    view.rerender(
      <WorkspaceStreamProvider workspaceId={WORKSPACE_ID} fleetIds={fleetIds}>
        {null}
      </WorkspaceStreamProvider>,
    );
    flushAnimationFrame();

    expect(view.queryByTestId(FLEET_A)).toBeNull();
  });

  it("a_dropped_frame_is_corrected_by_the_next_snapshot", () => {
    // The frame carrying 6 never arrives; the one carrying 7
    // does, and the tile reads 7 — the database's figure — with no reload:
    // every frame carries the whole truth, so nothing owed the missing one.
    const view = renderWall([FLEET_A]);
    const source = onlyEventSource();
    source.open();
    source.emit({ kind: FRAME_KIND.HELLO, fleet_ids: [FLEET_A] });
    source.emit(completionFrame(FLEET_A, "e5", COUNT_BEFORE_THE_DROP));
    flushAnimationFrame();
    expect(view.getByTestId(FLEET_A).textContent).toContain(`processed:${COUNT_BEFORE_THE_DROP}`);

    // e6's frame is the one transport lost.
    source.emit(completionFrame(FLEET_A, "e7", COUNT_AFTER_THE_DROP));
    flushAnimationFrame();

    expect(view.getByTestId(FLEET_A).textContent).toContain(`processed:${COUNT_AFTER_THE_DROP}`);
    expect(FakeEventSource.instances).toHaveLength(1);
  });

  it("a_capped_stream_degrades_to_a_snapshot_tile", () => {
    // A stream the daemon refused at its
    // ceiling (`SSE_MAX_STREAMS`, `UZ-API-002` — the admission is pinned by
    // `afd_api/tests/fleet_streams.rs`) errors at the EventSource; the tile
    // must then say `snapshot`, never a stale `live`.
    const view = renderWall([FLEET_A]);
    const source = onlyEventSource();
    source.open();
    flushAnimationFrame();
    expect(view.getByTestId(FLEET_A).textContent).toContain(LIVE_KIND_LABEL);

    source.fail();
    flushAnimationFrame();

    expect(view.getByTestId(FLEET_A).textContent).toContain(SNAPSHOT_KIND_LABEL);
    expect(view.getByTestId(FLEET_A).textContent).not.toContain(LIVE_KIND_LABEL);
  });

  it("cancels a queued tile notification when the provider unmounts", () => {
    const view = renderWall([FLEET_A]);
    const source = onlyEventSource();
    source.emit(activityFrame(FLEET_A));

    view.unmount();

    expect(cancelAnimationFrame).toHaveBeenCalledTimes(1);
  });
});
