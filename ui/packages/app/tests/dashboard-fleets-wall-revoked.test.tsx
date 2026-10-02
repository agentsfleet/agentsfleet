import { act, render } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { WorkspaceStreamProvider } from "@/components/domain/useWorkspaceStream";
import { ACCESS_REVOKED_LABEL } from "@/components/domain/FleetConnectionIndicator";
import { FRAME_KIND } from "@/lib/api/events-types";
import { ERROR_CODE } from "@/lib/errors";
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
const DESTRUCTIVE_TEXT = /\btext-destructive\b/;
const DESTRUCTIVE_DOT = /\bbg-destructive\b/;
const PULSE_DOT = /\bbg-pulse\b/;

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

    source.emit({ kind: FRAME_KIND.ACCESS_REVOKED, error_code: ERROR_CODE.AUTH_FORBIDDEN });
    source.fail();
    await act(async () => vi.advanceTimersByTimeAsync(PAST_EVERY_RETRY_MS));
    flushAnimationFrame();

    expect(view.getByTestId(BADGE).textContent).toBe(ACCESS_REVOKED_LABEL);
    expect(view.getByTestId(FLEET_A).textContent).toContain(SNAPSHOT_KIND_LABEL);
    expect(FakeEventSource.instances).toHaveLength(1);
  });

  // The badge sits in the wall's header row, beside the page title, where a
  // sentence overflowed at phone width and the pulse colour read as "live".
  it("should read the short label in the destructive tone, with a destructive dot rather than the pulse", () => {
    const view = render(
      <WorkspaceStreamProvider workspaceId={WORKSPACE_ID} fleetIds={[FLEET_A]}>
        <div data-testid={BADGE}>
          <WallLiveBadge liveTotal={1} />
        </div>
      </WorkspaceStreamProvider>,
    );
    const source = onlyEventSource();
    source.open();
    source.emit({ kind: FRAME_KIND.HELLO, fleet_ids: [FLEET_A] });
    flushAnimationFrame();
    const badge = view.getByRole("status");
    expect(badge.className).not.toMatch(DESTRUCTIVE_TEXT);
    expect(badge.firstElementChild?.className).toMatch(PULSE_DOT);

    source.emit({ kind: FRAME_KIND.ACCESS_REVOKED, error_code: ERROR_CODE.AUTH_FORBIDDEN });
    flushAnimationFrame();

    expect(badge.textContent).toBe(ACCESS_REVOKED_LABEL);
    expect(badge.className).toMatch(DESTRUCTIVE_TEXT);
    expect(badge.firstElementChild?.className).toMatch(DESTRUCTIVE_DOT);
    expect(badge.firstElementChild?.className).not.toMatch(PULSE_DOT);
  });
});
