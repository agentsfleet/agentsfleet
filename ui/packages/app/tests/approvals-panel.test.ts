import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";

const WORKSPACE_ID = "ws_panel_001";
const FLEET_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0aa701";
const TOKEN = "token_panel";

const { listApprovalsMock, listApprovalsActionMock } = vi.hoisted(() => ({
  listApprovalsMock: vi.fn(),
  listApprovalsActionMock: vi.fn(),
}));

vi.mock("@/lib/api/approvals", () => ({
  listApprovals: listApprovalsMock,
}));
// ApprovalsList (rendered inside the panel) imports its server actions; mock
// the module so the client-side polling effect doesn't try to call into
// Clerk's server-side auth() during the test.
vi.mock("@/app/(dashboard)/w/[workspaceId]/approvals/actions", () => ({
  listApprovalsAction: listApprovalsActionMock,
  approveApprovalAction: vi.fn(),
  denyApprovalAction: vi.fn(),
}));
vi.mock("next/link", () => ({
  default: ({ href, children, ...rest }: { href: string; children: React.ReactNode }) =>
    React.createElement("a", { href, ...rest }, children),
}));

import FleetApprovalsPanel from "@/components/domain/FleetApprovalsPanel";

beforeEach(() => {
  listApprovalsMock.mockResolvedValue({ items: [], next_cursor: null });
  listApprovalsActionMock.mockResolvedValue({
    ok: true,
    data: { items: [], next_cursor: null },
  });
});

afterEach(() => {
  cleanup();
  listApprovalsMock.mockReset();
  listApprovalsActionMock.mockReset();
});

describe("FleetApprovalsPanel — server-side fetch", () => {
  it("calls listApprovals with the fleetId scope and forwards items to the list", async () => {
    listApprovalsMock.mockResolvedValueOnce({
      items: [
        {
          gate_id: "01999999-1111-7000-8000-000000000001",
          fleet_id: FLEET_ID,
          fleet_name: "approvals-a",
          workspace_id: WORKSPACE_ID,
          action_id: "act_001",
          tool_name: "write_repo",
          action_name: "create_pr",
          gate_kind: "destructive_action",
          proposed_action: "Open PR titled X",
          evidence: {},
          blast_radius: "single repo branch",
          status: "pending",
          detail: "",
          requested_at: Date.now() - 60_000,
          timeout_at: Date.now() + 3_600_000,
          updated_at: null,
          resolved_by: "",
        },
      ],
      next_cursor: null,
    });

    const element = await FleetApprovalsPanel({
      workspaceId: WORKSPACE_ID,
      fleetId: FLEET_ID,
      token: TOKEN,
    });
    render(element);

    expect(listApprovalsMock).toHaveBeenCalledWith(
      WORKSPACE_ID,
      TOKEN,
      expect.objectContaining({ fleetId: FLEET_ID, limit: 50 }),
    );
    expect(screen.getByText("approvals-a")).toBeTruthy();
  });

  it("falls back to empty initial items when the upstream fetch rejects", async () => {
    listApprovalsMock.mockRejectedValueOnce(new Error("upstream 503"));
    const element = await FleetApprovalsPanel({
      workspaceId: WORKSPACE_ID,
      fleetId: FLEET_ID,
      token: TOKEN,
    });
    render(element);
    // Empty state appears when there are no items + no filter + no error.
    // Server-side rejection means the panel renders the EmptyState branch.
    expect(screen.getByText(/no pending approvals/i)).toBeTruthy();
  });
});
