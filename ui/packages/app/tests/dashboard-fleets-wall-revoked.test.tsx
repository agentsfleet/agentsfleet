import { act, render } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { WorkspaceStreamProvider } from "@/components/domain/conversation/stream/useWorkspaceStream";
import { FRAME_KIND } from "@/lib/api/events-types";
import { ERROR_CODE } from "@/lib/errors";
import {
  FakeEventSource,
  FleetProbe,
  flushAnimationFrame,
  onlyEventSource,
  setupWallTests,
  WORKSPACE_ID,
  WorkspaceConnectionProbe,
} from "./helpers/fleets-wall-harness";

// A member removed from the account while the wall is open: the daemon ends
// the wall's one stream with `access_revoked` and refuses the next request.

const FLEET_A = "fleet_a";
const CONNECTION = "workspace-connection";
const SNAPSHOT_KIND_LABEL = "kind:snapshot";
// Past the wall's capped backoff: a reconnect would have opened by now.
const PAST_EVERY_RETRY_MS = 60_000;

setupWallTests();

describe("the wall after access is revoked", () => {
  it("should say access is gone where it shows the connection, keep each tile's last activity, and never reconnect", async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
    const view = render(
      <WorkspaceStreamProvider workspaceId={WORKSPACE_ID} fleetIds={[FLEET_A]}>
        <WorkspaceConnectionProbe />
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

    expect(view.getByTestId(CONNECTION).textContent).toContain("revoked");
    expect(view.getByTestId(FLEET_A).textContent).toContain(SNAPSHOT_KIND_LABEL);
    expect(FakeEventSource.instances).toHaveLength(1);
  });

});
