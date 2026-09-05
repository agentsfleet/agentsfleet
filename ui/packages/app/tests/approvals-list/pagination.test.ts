import { AGENTSFLEET_A, AGENT_A_DISPLAY_NAME, ERR_ALREADY_RESOLVED, WORKSPACE_ID, approveApprovalActionMock, denyApprovalActionMock, gate, listApprovalsActionMock } from "./harness";
import React from "react";
import { describe, expect, it } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import ApprovalsList from "@/app/(dashboard)/w/[workspaceId]/approvals/components/ApprovalsList";

describe("ApprovalsList — loadMore", () => {
  it("surfaces an error and stops when loadMore fails", async () => {
    listApprovalsActionMock.mockResolvedValueOnce({
      ok: false,
      error: "load failed",
      errorCode: "UZ-APPROVAL-500",
    });
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: [gate()],
        initialCursor: "cursor_1",
      }),
    );
    fireEvent.click(screen.getByRole("button", { name: /load more/i }));
    await waitFor(() =>
      expect(listApprovalsActionMock).toHaveBeenCalledWith(
        WORKSPACE_ID,
        expect.objectContaining({ cursor: "cursor_1" }),
      ),
    );
    await waitFor(() => expect(screen.getByRole("alert")).toBeTruthy());
  });
});

describe("ApprovalsList — pagination", () => {
  it("shows Load more when initialCursor is set", () => {
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: [gate()],
        initialCursor: "cur_abc",
      }),
    );
    expect(screen.getByRole("button", { name: /load more/i })).toBeTruthy();
  });

  it("hides Load more when initialCursor is null", () => {
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: [gate()],
        initialCursor: null,
      }),
    );
    expect(screen.queryByRole("button", { name: /load more/i })).toBeNull();
  });

  it("appends items + advances cursor when Load more succeeds", async () => {
    listApprovalsActionMock.mockResolvedValueOnce({
      ok: true,
      data: {
        items: [
          gate({
            gate_id: "01999999-0000-7000-8000-000000000099",
            action_id: "act_099",
            fleet_name: "approvals-c",
          }),
        ],
        next_cursor: null,
      },
    });
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: [gate()],
        initialCursor: "cur_abc",
      }),
    );
    fireEvent.click(screen.getByRole("button", { name: /load more/i }));
    await waitFor(() => {
      expect(listApprovalsActionMock).toHaveBeenCalledWith(
        WORKSPACE_ID,
        expect.objectContaining({ cursor: "cur_abc", limit: 50 }),
      );
      expect(screen.getByText(AGENT_A_DISPLAY_NAME)).toBeTruthy();
      expect(screen.queryByRole("button", { name: /load more/i })).toBeNull();
    });
  });
});

describe("ApprovalsList — fleetId scoping", () => {
  it("passes fleetId to listApprovalsAction on Load more", async () => {
    listApprovalsActionMock.mockResolvedValueOnce({
      ok: true,
      data: { items: [], next_cursor: null },
    });
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        fleetId: AGENTSFLEET_A,
        initialItems: [gate()],
        initialCursor: "cur_abc",
      }),
    );
    fireEvent.click(screen.getByRole("button", { name: /load more/i }));
    await waitFor(() => {
      expect(listApprovalsActionMock).toHaveBeenCalledWith(
        WORKSPACE_ID,
        expect.objectContaining({ fleetId: AGENTSFLEET_A }),
      );
    });
  });
});

describe("ApprovalsList — branch coverage", () => {
  it("Load more shows Not authenticated when the action reports unauth", async () => {
    listApprovalsActionMock.mockResolvedValueOnce({
      ok: false,
      error: "Not authenticated",
      status: 401,
    });
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: [gate()],
        initialCursor: "cur_abc",
      }),
    );
    fireEvent.click(screen.getByRole("button", { name: /load more/i }));
    await waitFor(() => {
      expect(screen.getByRole("alert").textContent).toMatch(/not authenticated/i);
    });
  });

  it("Load more surfaces error when listApprovalsAction returns an upstream error", async () => {
    listApprovalsActionMock.mockResolvedValueOnce({ ok: false, error: "upstream 503" });
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: [gate()],
        initialCursor: "cur_abc",
      }),
    );
    fireEvent.click(screen.getByRole("button", { name: /load more/i }));
    await waitFor(() => {
      expect(screen.getByRole("alert").textContent).toMatch(/upstream 503/i);
    });
  });

  it("denyApproval already_resolved variant surfaces alert", async () => {
    denyApprovalActionMock.mockResolvedValueOnce({
      ok: true,
      data: {
        kind: "already_resolved",
        data: {
          gate_id: "01999999-0000-7000-8000-000000000001",
          action_id: "act_001",
          outcome: "denied",
          resolved_at: Date.now(),
          resolved_by: "slack:interaction",
          error_code: ERR_ALREADY_RESOLVED,
          detail: "raced",
        },
      },
    });
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: [gate()],
        initialCursor: null,
      }),
    );
    fireEvent.click(screen.getByRole("button", { name: /^deny$/i }));
    await waitFor(() => {
      expect(screen.getByRole("alert").textContent).toMatch(/already denied/i);
    });
  });

  it("filter empty + error still hides EmptyState (the fix to RULE WAUTH-style swallowing)", async () => {
    approveApprovalActionMock.mockResolvedValueOnce({ ok: false, error: "ECONNRESET" });
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: [gate()],
        initialCursor: null,
      }),
    );
    fireEvent.click(screen.getByRole("button", { name: /^approve$/i }));
    await waitFor(() => {
      expect(screen.getByRole("alert")).toBeTruthy();
      expect(screen.queryByText(/no pending approvals/i)).toBeNull();
    });
  });
});
