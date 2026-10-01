import { act, render } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { WorkspaceStreamProvider } from "@/components/domain/useWorkspaceStream";
import { ACCESS_REVOKED_MESSAGE } from "@/components/domain/FleetConnectionNotice";
import { FRAME_KIND } from "@/lib/api/events-types";
import WallLiveBadge from "@/app/(dashboard)/w/[workspaceId]/fleets/components/WallLiveBadge";
import {
  FakeEventSource,
  FleetProbe,
  flushAnimationFrame,
  onlyEventSource,
  setupWallTests,
  WORKSPACE_ID,
} from "./helpers/fleets-wall-harness";

// A member removed from the account while the wall is open: the daemon ends
// the wall's one stream with `access_revoked` and refuses the next request.

const FLEET_A = "fleet_a";
const BADGE = "badge";
const SNAPSHOT_KIND_LABEL = "kind:snapshot";
// Past the wall's capped backoff: a reconnect would have opened by now.
const PAST_EVERY_RETRY_MS = 60_000;

setupWallTests();

describe("the wall after access is revoked", () => {
  it("should say access is gone where it shows the connection, keep each tile's last activity, and never reconnect", async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
    const view = render(
      <WorkspaceStreamProvider workspaceId={WORKSPACE_ID} fleetIds={[FLEET_A]}>
        <div data-testid={BADGE}>
          <WallLiveBadge liveTotal={1} />
        </div>
        <FleetProbe fleetId={FLEET_A} />
      </WorkspaceStreamProvider>,
    );
    const source = onlyEventSource();
    source.open();
    source.emit({ kind: FRAME_KIND.HELLO, fleet_ids: [FLEET_A] });
    flushAnimationFrame();

    source.emit({ kind: FRAME_KIND.ACCESS_REVOKED, error_code: "UZ-AUTH-001" });
    source.fail();
    await act(async () => vi.advanceTimersByTimeAsync(PAST_EVERY_RETRY_MS));
    flushAnimationFrame();

    expect(view.getByTestId(BADGE).textContent).toBe(ACCESS_REVOKED_MESSAGE);
    expect(view.getByTestId(FLEET_A).textContent).toContain(SNAPSHOT_KIND_LABEL);
    expect(FakeEventSource.instances).toHaveLength(1);
  });
});
