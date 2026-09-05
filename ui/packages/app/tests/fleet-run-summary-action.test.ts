import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ApiError } from "@/lib/api/errors";
import {
  RUN_SUMMARY_APPROVALS_LIMIT,
  RUN_SUMMARY_LATEST_LIMIT,
} from "@/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/run-summary";

// getFleetRunSummaryAction composes three reads. The token and the three
// boundaries are the mocks; the builder and the action's own composition run
// for real, so what is asserted is the summary the strip will receive.

const { authMock, getFleetMock, listFleetEventsMock, listApprovalsMock } = vi.hoisted(() => ({
  authMock: vi.fn(),
  getFleetMock: vi.fn(),
  listFleetEventsMock: vi.fn(),
  listApprovalsMock: vi.fn(),
}));

vi.mock("@clerk/nextjs/server", () => ({ auth: authMock }));
vi.mock("@/lib/api/fleets", async (orig) => ({
  ...(await orig<typeof import("@/lib/api/fleets")>()),
  getFleet: getFleetMock,
}));
vi.mock("@/lib/api/events", async (orig) => ({
  ...(await orig<typeof import("@/lib/api/events")>()),
  listFleetEvents: listFleetEventsMock,
}));
vi.mock("@/lib/api/approvals", async (orig) => ({
  ...(await orig<typeof import("@/lib/api/approvals")>()),
  listApprovals: listApprovalsMock,
}));

import { getFleetRunSummaryAction } from "@/app/(dashboard)/w/[workspaceId]/fleets/actions";

const WORKSPACE_ID = "ws_1";
const FLEET_ID = "agt_1";
const TOKEN = "tok";
const STATUS_PAUSED = "paused";
const NEWEST_ROW = { event_id: "evt_9", tokens: 1200, status: "processed" };

beforeEach(() => {
  authMock.mockResolvedValue({ getToken: vi.fn().mockResolvedValue(TOKEN) });
  getFleetMock.mockResolvedValue({ fleet: { id: FLEET_ID, status: STATUS_PAUSED }, etag: '"e1"' });
  listFleetEventsMock.mockResolvedValue({ items: [NEWEST_ROW], next_cursor: "more" });
  listApprovalsMock.mockResolvedValue({ items: [{}, {}], next_cursor: null });
});
afterEach(() => {
  authMock.mockReset();
  getFleetMock.mockReset();
  listFleetEventsMock.mockReset();
  listApprovalsMock.mockReset();
});

describe("getFleetRunSummaryAction", () => {
  it("issues the three reads together and returns the strip's figures", async () => {
    let releaseFleet: (v: unknown) => void = () => {};
    getFleetMock.mockReturnValueOnce(
      new Promise((resolve) => {
        releaseFleet = resolve;
      }),
    );
    const pending = getFleetRunSummaryAction(WORKSPACE_ID, FLEET_ID);
    // Both secondary reads are on the wire while the fleet read is still pending.
    await vi.waitFor(() => expect(listApprovalsMock).toHaveBeenCalledTimes(1));
    expect(listFleetEventsMock).toHaveBeenCalledWith(WORKSPACE_ID, FLEET_ID, TOKEN, {
      limit: RUN_SUMMARY_LATEST_LIMIT,
    });
    expect(listApprovalsMock).toHaveBeenCalledWith(WORKSPACE_ID, TOKEN, {
      fleetId: FLEET_ID,
      limit: RUN_SUMMARY_APPROVALS_LIMIT,
    });
    releaseFleet({ fleet: { id: FLEET_ID, status: STATUS_PAUSED }, etag: '"e1"' });

    const result = await pending;
    expect(result).toEqual({
      ok: true,
      data: {
        status: STATUS_PAUSED,
        latest: NEWEST_ROW,
        latestAvailable: true,
        pendingApprovals: 2,
        pendingApprovalsHasMore: false,
        approvalsAvailable: true,
      },
    });
  });

  it("a failed fleet read fails the whole summary with the wire status and code", async () => {
    getFleetMock.mockRejectedValueOnce(new ApiError("gone", 404, "UZ-AGT-009", "req_1"));
    const result = await getFleetRunSummaryAction(WORKSPACE_ID, FLEET_ID);
    expect(result).toEqual({ ok: false, error: "gone", status: 404, errorCode: "UZ-AGT-009" });
  });

  it("a failed events read leaves the latest figures unavailable and the rest intact", async () => {
    listFleetEventsMock.mockRejectedValueOnce(new ApiError("down", 503, "UZ-API-002"));
    const result = await getFleetRunSummaryAction(WORKSPACE_ID, FLEET_ID);
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.data.latest).toBeNull();
    expect(result.data.latestAvailable).toBe(false);
    expect(result.data.pendingApprovals).toBe(2);
    expect(result.data.approvalsAvailable).toBe(true);
  });

  it("a failed approvals read leaves the approvals unavailable and the latest figures intact", async () => {
    listApprovalsMock.mockRejectedValueOnce(new TypeError("fetch failed"));
    const result = await getFleetRunSummaryAction(WORKSPACE_ID, FLEET_ID);
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.data.approvalsAvailable).toBe(false);
    expect(result.data.pendingApprovals).toBe(0);
    expect(result.data.latest).toEqual(NEWEST_ROW);
  });

  it("no session token is a 401 before any read is issued", async () => {
    authMock.mockResolvedValueOnce({ getToken: vi.fn().mockResolvedValue(null) });
    const result = await getFleetRunSummaryAction(WORKSPACE_ID, FLEET_ID);
    expect(result).toMatchObject({ ok: false, status: 401 });
    expect(getFleetMock).not.toHaveBeenCalled();
    expect(listFleetEventsMock).not.toHaveBeenCalled();
  });
});
