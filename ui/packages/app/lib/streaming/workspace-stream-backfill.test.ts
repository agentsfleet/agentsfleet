import { describe, expect, it, vi } from "vitest";

import { FRAME_KIND } from "@/lib/api/events-types";
import {
  onlyEventSource,
  settlePromises,
  setupWallTests,
  WORKSPACE_ID,
} from "@/tests/helpers/fleets-wall-harness";
import { noteServerFrameTime, subscribeWorkspaceFrames, type BackfillFn } from "./workspace-stream";

setupWallTests();

// Where the wall's anchor stands once the first walk's rows are confirmed.
const ADVANCED_MS = 1_700_000_000_000;
const IDLE_RELEASE_MS = 30_000;

// A connected wall whose first gap starts a walk that hangs until released,
// and `gaps` more gaps that arrive while it hangs. The walk confirms its rows
// as the wall does, so a follow-up starts from the advanced anchor.
function gapsDuringWalk(gaps: number) {
  const first = Promise.withResolvers<void>();
  const backfill = vi.fn<BackfillFn>(async () => {});
  backfill.mockImplementationOnce(async () => {
    await first.promise;
    noteServerFrameTime(WORKSPACE_ID, ADVANCED_MS);
  });
  const release = subscribeWorkspaceFrames(WORKSPACE_ID, () => {}, backfill);
  const source = onlyEventSource();
  source.open();
  source.emit({ kind: FRAME_KIND.CATCHING_UP, dropped: 0 });
  for (let gap = 0; gap < gaps; gap += 1) source.emit({ kind: FRAME_KIND.CATCHING_UP, dropped: 0 });
  expect(backfill).toHaveBeenCalledTimes(1);
  return { backfill, release, finishFirstWalk: () => first.resolve() };
}

describe("a gap reported while the wall is already backfilling", () => {
  it("test_wall_shows_a_gap — a gap during a walk runs a second walk, from the advanced anchor, after it", async () => {
    const { backfill, finishFirstWalk } = gapsDuringWalk(1);
    finishFirstWalk();
    await settlePromises();
    expect(backfill).toHaveBeenCalledTimes(2);
    expect(backfill).toHaveBeenLastCalledWith(WORKSPACE_ID, ADVANCED_MS);
  });

  it("test_wall_shows_a_gap — three gaps during a walk still queue one walk after it", async () => {
    const { backfill, finishFirstWalk } = gapsDuringWalk(3);
    finishFirstWalk();
    await settlePromises();
    expect(backfill).toHaveBeenCalledTimes(2);
  });

  it("test_wall_shows_a_gap — a connection torn down during its walk queues nothing after it", async () => {
    vi.useFakeTimers();
    const { backfill, release, finishFirstWalk } = gapsDuringWalk(1);
    release();
    vi.advanceTimersByTime(IDLE_RELEASE_MS);
    finishFirstWalk();
    await settlePromises();
    expect(backfill).toHaveBeenCalledTimes(1);
  });
});
