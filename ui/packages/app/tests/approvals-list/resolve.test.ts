import { AGENT_A_DISPLAY_NAME, ERR_ALREADY_RESOLVED, WORKSPACE_ID, approveApprovalActionMock, denyApprovalActionMock, gate } from "./harness";
import React from "react";
import { describe, expect, it } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import ApprovalsList from "@/app/(dashboard)/w/[workspaceId]/approvals/components/ApprovalsList";

describe("ApprovalsList — resolve actions", () => {
  it("an inbox row leaves before the resolve settles and returns on failure", async () => {
    let settle: (result: { ok: false; error: string; status: number }) => void = () => {};
    approveApprovalActionMock.mockReturnValueOnce(
      new Promise((resolve) => {
        settle = resolve;
      }),
    );
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: [gate()],
        initialCursor: null,
      }),
    );
    fireEvent.click(screen.getByRole("button", { name: /^approve$/i }));
    // Gone at the click, while the action is still in flight.
    await waitFor(() => expect(screen.queryByText(AGENT_A_DISPLAY_NAME)).toBeNull());
    expect(approveApprovalActionMock).toHaveBeenCalledWith(
      WORKSPACE_ID,
      "01999999-0000-7000-8000-000000000001",
    );

    settle({ ok: false, error: "gate service unavailable", status: 503 });

    // The refusal puts the row back beside the error.
    await waitFor(() => expect(screen.getByText(AGENT_A_DISPLAY_NAME)).toBeTruthy());
    expect(screen.getByRole("alert").textContent).toMatch(/gate service unavailable/);
  });

  it("a rejected resolve call restores the row with the error, never the error boundary", async () => {
    approveApprovalActionMock.mockRejectedValueOnce(new Error("Server Component transport failed"));
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: [gate()],
        initialCursor: null,
      }),
    );
    fireEvent.click(screen.getByRole("button", { name: /^approve$/i }));

    // The transport, not the gate, refused: the row is back beside the message.
    await waitFor(() => expect(screen.getByText(AGENT_A_DISPLAY_NAME)).toBeTruthy());
    expect(screen.getByRole("alert").textContent).toMatch(/transport failed/);
  });

  it("a resolve call rejected with something that is not an Error still names it", async () => {
    approveApprovalActionMock.mockRejectedValueOnce("the transport gave up");
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: [gate()],
        initialCursor: null,
      }),
    );
    fireEvent.click(screen.getByRole("button", { name: /^approve$/i }));

    await waitFor(() => expect(screen.getByText(AGENT_A_DISPLAY_NAME)).toBeTruthy());
    expect(screen.getByRole("alert").textContent).toMatch(/transport gave up/);
  });

  it("resolving the last gate keeps the list shell until the server confirms", async () => {
    let settle: (result: { ok: true; data: unknown }) => void = () => {};
    approveApprovalActionMock.mockReturnValueOnce(
      new Promise((resolve) => {
        settle = resolve;
      }),
    );
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: [gate()],
        initialCursor: null,
      }),
    );
    fireEvent.click(screen.getByRole("button", { name: /^approve$/i }));
    await waitFor(() => expect(screen.queryByText(AGENT_A_DISPLAY_NAME)).toBeNull());

    // Gone from view, but "No pending approvals" is the server's claim to make:
    // the filter input stays and the status region is not announced yet.
    expect(screen.queryByText("No pending approvals")).toBeNull();
    expect(screen.getByRole("searchbox", { name: /filter approvals/i })).toBeTruthy();

    settle({
      ok: true,
      data: {
        kind: "resolved",
        data: {
          gate_id: "01999999-0000-7000-8000-000000000001",
          action_id: "act_001",
          outcome: "approved",
          resolved_at: Date.now(),
          resolved_by: "user:user_abc",
        },
      },
    });
    await waitFor(() => expect(screen.getByText("No pending approvals")).toBeTruthy());
  });

  it("a resolved row stays gone once the server confirms", async () => {
    approveApprovalActionMock.mockResolvedValueOnce({
      ok: true,
      data: {
        kind: "resolved",
        data: {
          gate_id: "01999999-0000-7000-8000-000000000001",
          action_id: "act_001",
          outcome: "approved",
          resolved_at: Date.now(),
          resolved_by: "user:user_abc",
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
    fireEvent.click(screen.getByRole("button", { name: /^approve$/i }));
    await waitFor(() => expect(approveApprovalActionMock).toHaveBeenCalled());
    // The base list dropped it too, so the settled transition shows no row.
    await waitFor(() => expect(screen.queryByText(AGENT_A_DISPLAY_NAME)).toBeNull());
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("optimistically removes a row when denyApprovalAction returns kind=resolved", async () => {
    denyApprovalActionMock.mockResolvedValueOnce({
      ok: true,
      data: {
        kind: "resolved",
        data: {
          gate_id: "01999999-0000-7000-8000-000000000001",
          action_id: "act_001",
          outcome: "denied",
          resolved_at: Date.now(),
          resolved_by: "user:user_abc",
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
      expect(denyApprovalActionMock).toHaveBeenCalled();
      expect(screen.queryByText(AGENT_A_DISPLAY_NAME)).toBeNull();
    });
  });

  it("an already resolved gate stays removed and shows who resolved it", async () => {
    approveApprovalActionMock.mockResolvedValueOnce({
      ok: true,
      data: {
        kind: "already_resolved",
        data: {
          gate_id: "01999999-0000-7000-8000-000000000001",
          action_id: "act_001",
          outcome: "approved",
          resolved_at: Date.now(),
          resolved_by: "slack:webhook",
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
    fireEvent.click(screen.getByRole("button", { name: /^approve$/i }));
    await waitFor(() => {
      const alert = screen.getByRole("alert");
      expect(alert.textContent).toMatch(/already approved/i);
      expect(alert.textContent).toMatch(/slack:webhook/);
    });
    // Resolved elsewhere is still resolved: the pending inbox has no row for it.
    expect(screen.queryByText(AGENT_A_DISPLAY_NAME)).toBeNull();
  });

  it("shows an error when the server action reports unauth", async () => {
    approveApprovalActionMock.mockResolvedValueOnce({
      ok: false,
      error: "Not authenticated",
      status: 401,
    });
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: [gate()],
        initialCursor: null,
      }),
    );
    fireEvent.click(screen.getByRole("button", { name: /^approve$/i }));
    await waitFor(() => {
      expect(screen.getByRole("alert").textContent).toMatch(/not authenticated/i);
    });
    // A refused resolve leaves the row where it was. The error state lands
    // before the transition settles, so the restored row is awaited, not read.
    await waitFor(() => expect(screen.getByText(AGENT_A_DISPLAY_NAME)).toBeTruthy());
  });

  it("renders error message when approveApprovalAction surfaces a network error", async () => {
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
      expect(screen.getByRole("alert").textContent).toMatch(/ECONNRESET/);
    });
  });

  it("falls back to presentError default when the action returns an empty error", async () => {
    approveApprovalActionMock.mockResolvedValueOnce({ ok: false, error: "" });
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: [gate()],
        initialCursor: null,
      }),
    );
    fireEvent.click(screen.getByRole("button", { name: /^approve$/i }));
    // WS-G — empty server error falls through presentError's default path.
    await waitFor(() => {
      expect(screen.getByRole("alert").textContent).toMatch(/Couldn't approve this request/i);
    });
  });

  // Deny-arm pin so patch coverage hits both ternary branches (the approve
  // arm above stays load-bearing on its own).
  it("deny error path renders 'Couldn't deny this request' (WS-G verb literal — deny arm)", async () => {
    denyApprovalActionMock.mockResolvedValueOnce({ ok: false, error: "" });
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: [gate()],
        initialCursor: null,
      }),
    );
    fireEvent.click(screen.getByRole("button", { name: /^deny$/i }));
    await waitFor(() => {
      expect(screen.getByRole("alert").textContent).toMatch(/Couldn't deny this request/i);
    });
  });
});
