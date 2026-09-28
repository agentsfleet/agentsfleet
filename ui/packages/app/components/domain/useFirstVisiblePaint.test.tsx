import { cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { FIRST_VISIBLE_MEASURE, useFirstVisiblePaint } from "./useFirstVisiblePaint";

const SUBMITTED_AT_MS = 1_000;
// Frames run at once, so the two-frame wait resolves inside the effect.
const frames = vi.fn((callback: FrameRequestCallback) => {
  callback(0);
  return 1;
});

beforeEach(() => {
  vi.stubGlobal("requestAnimationFrame", frames);
  vi.stubGlobal("cancelAnimationFrame", () => undefined);
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe("useFirstVisiblePaint", () => {
  it("measures a reply once, even when its row remounts with a fresh ref", () => {
    const measure = vi.spyOn(performance, "measure").mockImplementation(() => ({}) as PerformanceMeasure);
    const first = renderHook(() => useFirstVisiblePaint("ev-remount", SUBMITTED_AT_MS, true));
    expect(measure).toHaveBeenCalledTimes(1);
    expect(measure).toHaveBeenCalledWith(FIRST_VISIBLE_MEASURE, expect.objectContaining({ start: SUBMITTED_AT_MS }));
    first.unmount();
    frames.mockClear();

    // A regroup or a navigation back mounts the row again: already measured,
    // so it does not even wait for a frame.
    renderHook(() => useFirstVisiblePaint("ev-remount", SUBMITTED_AT_MS, true));
    expect(measure).toHaveBeenCalledTimes(1);
    expect(frames).not.toHaveBeenCalled();
  });
});
