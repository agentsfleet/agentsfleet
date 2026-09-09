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

import { APPROVAL_STATUS_ORDER } from "@/lib/api/approvals-types";
import {
  listAllApprovalsAction,
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
  // The whole point of this action: the statuses fan out on the SERVER, under
  // one token. Five calls from the client were five sequential round trips,
  // because Next runs Server Actions one at a time per client.
  it("listAllApprovalsAction reads every status in the order under one token", async () => {
    listApprovalsMock.mockResolvedValue({ items: [], next_cursor: null });
    await listAllApprovalsAction("ws-1");
    expect(listApprovalsMock).toHaveBeenCalledTimes(APPROVAL_STATUS_ORDER.length);
    for (const status of APPROVAL_STATUS_ORDER) {
      expect(listApprovalsMock).toHaveBeenCalledWith("ws-1", "tok", { status });
    }
  });

  it("listAllApprovalsAction narrows to the statuses it is given and threads opts", async () => {
    listApprovalsMock.mockResolvedValue({ items: [], next_cursor: null });
    await listAllApprovalsAction("ws-1", { fleetId: "z-1", limit: 25 }, ["denied"]);
    expect(listApprovalsMock).toHaveBeenCalledTimes(1);
    expect(listApprovalsMock).toHaveBeenCalledWith("ws-1", "tok", {
      fleetId: "z-1",
      limit: 25,
      status: "denied",
    });
  });

  it("listAllApprovalsAction merges the pages into one list", async () => {
    listApprovalsMock
      .mockResolvedValueOnce({ items: [{ gate_id: "a" }], next_cursor: null })
      .mockResolvedValueOnce({ items: [{ gate_id: "b" }], next_cursor: "cur-9" });
    const r = await listAllApprovalsAction("ws-1", {}, ["pending", "denied"]);
    // The merged list has no single cursor to page from; the caller reads whole
    // pages per status, so claiming one would be claiming a position nothing holds.
    expect(r).toEqual({
      ok: true,
      data: { items: [{ gate_id: "a" }, { gate_id: "b" }], next_cursor: null },
    });
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
