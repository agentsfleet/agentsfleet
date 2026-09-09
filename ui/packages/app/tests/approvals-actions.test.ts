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
  // The whole point of this action: ONE read, with no status, which the API
  // answers from a single query. It fanned out over five statuses until M194 —
  // first from the client (five sequential round trips, because Next runs
  // Server Actions one at a time), then on the server (one call, five queries).
  it("listApprovalsAction reads once, naming no status", async () => {
    listApprovalsMock.mockResolvedValue({ items: [], next_cursor: null });
    await listApprovalsAction("ws-1");
    expect(listApprovalsMock).toHaveBeenCalledTimes(1);
    expect(listApprovalsMock).toHaveBeenCalledWith("ws-1", "tok", {});
    // An omitted status is the filter being off. Sending one would narrow the
    // page back to a single state and put the other four back behind more reads.
    const [, , opts] = listApprovalsMock.mock.calls[0] as [string, string, Record<string, unknown>];
    expect(opts).not.toHaveProperty("status");
  });

  it("listApprovalsAction threads the opts it is given", async () => {
    listApprovalsMock.mockResolvedValue({ items: [], next_cursor: null });
    await listApprovalsAction("ws-1", { fleetId: "z-1", limit: 25 });
    expect(listApprovalsMock).toHaveBeenCalledTimes(1);
    expect(listApprovalsMock).toHaveBeenCalledWith("ws-1", "tok", {
      fleetId: "z-1",
      limit: 25,
    });
  });

  it("listApprovalsAction returns the page as the API gave it, cursor included", async () => {
    // The cursor survives now. The fan-out had to discard it — five pages have
    // no single position to resume from — so paging was dead behind this action.
    listApprovalsMock.mockResolvedValueOnce({ items: [{ gate_id: "a" }], next_cursor: "cur-9" });
    const r = await listApprovalsAction("ws-1");
    expect(r).toEqual({
      ok: true,
      data: { items: [{ gate_id: "a" }], next_cursor: "cur-9" },
    });
  });

  it("listApprovalsAction surfaces a refusal instead of an empty page", async () => {
    // withToken owns the envelope; a failed read must not read as "no approvals".
    withTokenMock.mockResolvedValueOnce({ ok: false, error: "upstream is down", status: 503 });
    const r = await listApprovalsAction("ws-1");
    expect(r).toEqual({ ok: false, error: "upstream is down", status: 503 });
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
