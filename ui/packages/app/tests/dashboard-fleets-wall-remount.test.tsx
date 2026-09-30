import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { WorkspaceStreamProvider } from "@/components/domain/useWorkspaceStream";
import { FRAME_KIND } from "@/lib/api/events-types";
import WallLiveBadge from "@/app/(dashboard)/w/[workspaceId]/fleets/components/WallLiveBadge";
import {
  FakeEventSource,
  FleetProbe,
  flushAnimationFrame,
  LAST_KNOWN_LABEL,
  LIVE_LABEL,
  onlyEventSource,
  setupWallTests,
  SPENT_NANOS,
  WORKSPACE_ID,
} from "./helpers/fleets-wall-harness";

// Leaving the wall and coming back inside the idle grace reuses its connection,
// whose `hello` went by before the new wall mounted. On app-dev that wall read
// "connecting…" for good; it must read live at once, on the same connection,
// without the counters that greeting carried.

const FLEET_A = "fleet_a";
const FLEET_B = "fleet_b";
const FLEET_IDS = [FLEET_A, FLEET_B];
const BADGE = "badge";
const CONNECTING = "connecting…";
const LIVE = `${FLEET_IDS.length} live`;
const EVENTS_PROCESSED = 5;
const COUNTED = `processed:${EVENTS_PROCESSED}`;
const UNCOUNTED = "processed:none";

setupWallTests();

function wall(fleetIds: string[]) {
  return (
    <WorkspaceStreamProvider workspaceId={WORKSPACE_ID} fleetIds={fleetIds}>
      <div data-testid={BADGE}>
        <WallLiveBadge liveTotal={FLEET_IDS.length} />
      </div>
      {fleetIds.map((fleetId) => (
        <FleetProbe key={fleetId} fleetId={fleetId} />
      ))}
    </WorkspaceStreamProvider>
  );
}

function greetOnlyFleetA() {
  const source = onlyEventSource();
  source.open();
  source.emit({
    kind: FRAME_KIND.HELLO,
    fleet_ids: [FLEET_A],
    counters: { [FLEET_A]: { events_processed: EVENTS_PROCESSED, budget_used_nanos: SPENT_NANOS } },
  });
  flushAnimationFrame();
}

describe("the wall comes back inside the idle grace", () => {
  it("reads live at once on the same connection, without the old counters", () => {
    const first = render(wall(FLEET_IDS));
    expect(first.getByTestId(BADGE).textContent).toBe(CONNECTING);
    greetOnlyFleetA();
    expect(first.getByTestId(BADGE).textContent).toBe(LIVE);
    expect(first.getByTestId(FLEET_A).textContent).toContain(COUNTED);
    first.unmount();

    const again = render(wall(FLEET_IDS));
    flushAnimationFrame();

    expect(again.getByTestId(BADGE).textContent).toBe(LIVE);
    expect(again.getByTestId(FLEET_A).textContent).toContain(LIVE_LABEL);
    expect(again.getByTestId(FLEET_A).textContent).toContain(UNCOUNTED);
    expect(again.getByTestId(FLEET_B).textContent).toContain(LAST_KNOWN_LABEL);
    expect(FakeEventSource.instances).toHaveLength(1);
    expect(onlyEventSource().closed).toBe(false);
  });

  it("keeps its connection and its reading when only the fleet set changes", () => {
    const view = render(wall([FLEET_A]));
    greetOnlyFleetA();

    view.rerender(wall(FLEET_IDS));
    flushAnimationFrame();

    expect(view.getByTestId(BADGE).textContent).toBe(LIVE);
    expect(view.getByTestId(FLEET_A).textContent).toContain(COUNTED);
    expect(FakeEventSource.instances).toHaveLength(1);
    expect(onlyEventSource().closed).toBe(false);
  });
});
