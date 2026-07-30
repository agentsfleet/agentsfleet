import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render } from "@testing-library/react";

const captureMock = vi.fn();
vi.mock("@/lib/analytics/posthog", () => ({
  captureProductEvent: (...args: unknown[]) => captureMock(...args),
}));

import { EVENTS } from "@/lib/analytics/events";
import { RunnerViewedTracker } from "./RunnerViewedTracker";

afterEach(() => cleanup());
beforeEach(() => captureMock.mockReset());

describe("RunnerViewedTracker", () => {
  it("test_runner_viewed_event_properties", () => {
    const { rerender } = render(
      <RunnerViewedTracker runnerId="r-1" liveness="busy" adminState="active" />,
    );
    expect(captureMock).toHaveBeenCalledTimes(1);
    const [event, props] = captureMock.mock.calls[0]!;
    expect(event).toBe(EVENTS.runner_viewed);
    expect(props).toEqual({ runner_id: "r-1", liveness: "busy", admin_state: "active" });
    // Coarse states only — no host identifier, token or label values.
    expect(Object.keys(props)).toEqual(["runner_id", "liveness", "admin_state"]);

    // A re-render of the same runner does not double-fire.
    rerender(<RunnerViewedTracker runnerId="r-1" liveness="online" adminState="active" />);
    expect(captureMock).toHaveBeenCalledTimes(1);
  });
});
