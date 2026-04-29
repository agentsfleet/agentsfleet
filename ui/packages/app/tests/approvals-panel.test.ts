import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";

const WORKSPACE_ID = "ws_panel_001";
const ZOMBIE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0aa701";
const TOKEN = "token_panel";

const { listApprovalsMock, approveApprovalMock, denyApprovalMock, getTokenFn } =
  vi.hoisted(() => ({
    listApprovalsMock: vi.fn(),
    approveApprovalMock: vi.fn(),
    denyApprovalMock: vi.fn(),
    getTokenFn: vi.fn(),
  }));

vi.mock("@/lib/api/approvals", () => ({
  listApprovals: listApprovalsMock,
  approveApproval: approveApprovalMock,
  denyApproval: denyApprovalMock,
}));
vi.mock("@/lib/auth/client", () => ({
  useClientToken: () => ({ getToken: getTokenFn }),
}));
vi.mock("next/link", () => ({
  default: ({ href, children, ...rest }: { href: string; children: React.ReactNode }) =>
    React.createElement("a", { href, ...rest }, children),
}));

import ZombieApprovalsPanel from "@/components/domain/ZombieApprovalsPanel";

beforeEach(() => {
  getTokenFn.mockResolvedValue(TOKEN);
  listApprovalsMock.mockResolvedValue({ items: [], next_cursor: null });
});

afterEach(() => {
  cleanup();
  listApprovalsMock.mockReset();
  approveApprovalMock.mockReset();
  denyApprovalMock.mockReset();
  getTokenFn.mockReset();
});

describe("ZombieApprovalsPanel — server-side fetch", () => {
  it("calls listApprovals with the zombieId scope and forwards items to the list", async () => {
    listApprovalsMock.mockResolvedValueOnce({
      items: [
        {
          gate_id: "01999999-1111-7000-8000-000000000001",
          zombie_id: ZOMBIE_ID,
          zombie_name: "approvals-a",
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

    const element = await ZombieApprovalsPanel({
      workspaceId: WORKSPACE_ID,
      zombieId: ZOMBIE_ID,
      token: TOKEN,
    });
    render(element);

    expect(listApprovalsMock).toHaveBeenCalledWith(
      WORKSPACE_ID,
      TOKEN,
      expect.objectContaining({ zombieId: ZOMBIE_ID, limit: 50 }),
    );
    expect(screen.getByText("approvals-a")).toBeTruthy();
  });

  it("falls back to empty initial items when the upstream fetch rejects", async () => {
    listApprovalsMock.mockRejectedValueOnce(new Error("upstream 503"));
    const element = await ZombieApprovalsPanel({
      workspaceId: WORKSPACE_ID,
      zombieId: ZOMBIE_ID,
      token: TOKEN,
    });
    render(element);
    // Empty state appears when there are no items + no filter + no error.
    // Server-side rejection means the panel renders the EmptyState branch.
    expect(screen.getByText(/no pending approvals/i)).toBeTruthy();
  });
});
