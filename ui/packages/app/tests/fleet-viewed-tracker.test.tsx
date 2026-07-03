import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render } from "@testing-library/react";

const { captureProductEventMock } = vi.hoisted(() => ({ captureProductEventMock: vi.fn() }));
vi.mock("@/lib/analytics/posthog", () => ({ captureProductEvent: captureProductEventMock }));

import { FleetViewedTracker } from "../app/(dashboard)/fleets/[id]/components/FleetViewedTracker";
import { EVENTS } from "../lib/analytics/events";

afterEach(() => {
  cleanup();
  captureProductEventMock.mockReset();
});

describe("FleetViewedTracker", () => {
  it("fires fleet_viewed once with the fleet id + status on mount", () => {
    render(React.createElement(FleetViewedTracker, { fleetId: "zom_1", status: "active" }));
    expect(captureProductEventMock).toHaveBeenCalledTimes(1);
    expect(captureProductEventMock).toHaveBeenCalledWith(EVENTS.fleet_viewed, {
      fleet_id: "zom_1",
      status: "active",
    });
  });

  it("does not re-fire when the same fleet re-renders with a new status", () => {
    const { rerender } = render(
      React.createElement(FleetViewedTracker, { fleetId: "zom_1", status: "active" }),
    );
    rerender(React.createElement(FleetViewedTracker, { fleetId: "zom_1", status: "paused" }));
    expect(captureProductEventMock).toHaveBeenCalledTimes(1);
  });
});
