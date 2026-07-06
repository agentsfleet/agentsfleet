import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// ── Shared mocks ───────────────────────────────────────────────────────────
// These approval server actions are thin forwarders: each wraps the API client
// in withToken((t) => apiFn(args, t, ...)). We mock the token wrapper and the
// API client so the only thing under test is the forwarding — that each export
// threads the token into the position the source uses and returns the wrapped
// {ok:true,data} envelope (the real boundary is the backend, proven by the
// integration suite).

// vi.mock is hoisted above the static actions import, so the mock fns must be
// created via vi.hoisted() to exist when the factories run.
const { withTokenMock, listApprovalsMock, approveApprovalMock, denyApprovalMock } = vi.hoisted(() => ({
  withTokenMock: vi.fn(),
  listApprovalsMock: vi.fn(),
  approveApprovalMock: vi.fn(),
  denyApprovalMock: vi.fn(),
}));

vi.mock("@/lib/actions/with-token", () => ({ withToken: withTokenMock }));
vi.mock("@/lib/api/approvals", () => ({
  listApprovals: listApprovalsMock,
  approveApproval: approveApprovalMock,
  denyApproval: denyApprovalMock,
}));

import {
  listApprovalsAction,
  approveApprovalAction,
  denyApprovalAction,
} from "@/app/(dashboard)/w/[workspaceId]/approvals/actions";

beforeEach(() => {
  vi.clearAllMocks();
  // withToken just forwards a resolved token to its callback for the happy path.
  withTokenMock.mockImplementation(async (fn: (t: string) => Promise<unknown>) => ({
    ok: true,
    data: await fn("tok"),
  }));
});
afterEach(() => vi.resetAllMocks());

describe("approval server actions — thin forwarders", () => {
  it("listApprovalsAction forwards workspaceId + token + default opts to the client", async () => {
    listApprovalsMock.mockResolvedValueOnce({ items: [], next_cursor: null });
    const r = await listApprovalsAction("ws-1");
    expect(r).toEqual({ ok: true, data: { items: [], next_cursor: null } });
    // opts defaults to {} in the source signature, threaded as the third arg.
    expect(listApprovalsMock).toHaveBeenCalledWith("ws-1", "tok", {});
  });

  it("listApprovalsAction threads explicit opts through to the client", async () => {
    listApprovalsMock.mockResolvedValueOnce({ items: [], next_cursor: "cur-9" });
    const opts = { status: "pending", fleetId: "z-1", limit: 25 };
    const r = await listApprovalsAction("ws-1", opts);
    expect(r).toEqual({ ok: true, data: { items: [], next_cursor: "cur-9" } });
    expect(listApprovalsMock).toHaveBeenCalledWith("ws-1", "tok", opts);
  });

  it("approveApprovalAction forwards workspaceId + gateId + token with an explicit reason", async () => {
    const outcome = { kind: "resolved", data: { gate_id: "g-1" } };
    approveApprovalMock.mockResolvedValueOnce(outcome);
    const r = await approveApprovalAction("ws-1", "g-1", "looks safe");
    expect(r).toEqual({ ok: true, data: outcome });
    // token is the third arg; reason is threaded in the fourth position.
    expect(approveApprovalMock).toHaveBeenCalledWith("ws-1", "g-1", "tok", "looks safe");
  });

  it("approveApprovalAction forwards an undefined reason when none is given", async () => {
    const outcome = { kind: "resolved", data: { gate_id: "g-2" } };
    approveApprovalMock.mockResolvedValueOnce(outcome);
    const r = await approveApprovalAction("ws-1", "g-2");
    expect(r).toEqual({ ok: true, data: outcome });
    expect(approveApprovalMock).toHaveBeenCalledWith("ws-1", "g-2", "tok", undefined);
  });

  it("denyApprovalAction forwards workspaceId + gateId + token with an explicit reason", async () => {
    const outcome = { kind: "resolved", data: { gate_id: "g-3" } };
    denyApprovalMock.mockResolvedValueOnce(outcome);
    const r = await denyApprovalAction("ws-1", "g-3", "blast radius too wide");
    expect(r).toEqual({ ok: true, data: outcome });
    expect(denyApprovalMock).toHaveBeenCalledWith("ws-1", "g-3", "tok", "blast radius too wide");
  });

  it("denyApprovalAction forwards an undefined reason when none is given", async () => {
    const outcome = { kind: "already_resolved", data: { gate_id: "g-4" } };
    denyApprovalMock.mockResolvedValueOnce(outcome);
    const r = await denyApprovalAction("ws-1", "g-4");
    expect(r).toEqual({ ok: true, data: outcome });
    expect(denyApprovalMock).toHaveBeenCalledWith("ws-1", "g-4", "tok", undefined);
  });
});
