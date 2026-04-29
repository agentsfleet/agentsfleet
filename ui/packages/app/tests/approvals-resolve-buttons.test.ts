import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

const WORKSPACE_ID = "ws_resolve_001";
const GATE_ID = "01999999-0000-7000-8000-000000000001";
const TOKEN = "token_xyz";
const ERR_ALREADY_RESOLVED = "UZ-APPROVAL-006" as const;

const { getTokenFn, approveApprovalMock, denyApprovalMock, routerPush, routerRefresh } =
  vi.hoisted(() => ({
    getTokenFn: vi.fn(),
    approveApprovalMock: vi.fn(),
    denyApprovalMock: vi.fn(),
    routerPush: vi.fn(),
    routerRefresh: vi.fn(),
  }));

vi.mock("next/navigation", () => ({
  useRouter: () => ({ push: routerPush, refresh: routerRefresh }),
}));
vi.mock("@/lib/auth/client", () => ({
  useClientToken: () => ({ getToken: getTokenFn }),
}));
vi.mock("@/lib/api/approvals", () => ({
  approveApproval: approveApprovalMock,
  denyApproval: denyApprovalMock,
}));

import ResolveButtons from "@/app/(dashboard)/approvals/[gateId]/ResolveButtons";

beforeEach(() => {
  getTokenFn.mockResolvedValue(TOKEN);
});

afterEach(() => {
  cleanup();
  approveApprovalMock.mockReset();
  denyApprovalMock.mockReset();
  getTokenFn.mockReset();
  routerPush.mockReset();
  routerRefresh.mockReset();
});

describe("ResolveButtons — rendering", () => {
  it("renders reason textarea + approve + deny", () => {
    render(
      React.createElement(ResolveButtons, { workspaceId: WORKSPACE_ID, gateId: GATE_ID }),
    );
    expect(screen.getByLabelText(/reason/i)).toBeTruthy();
    expect(screen.getByRole("button", { name: /^approve$/i })).toBeTruthy();
    expect(screen.getByRole("button", { name: /^deny$/i })).toBeTruthy();
  });
});

describe("ResolveButtons — approve happy path", () => {
  it("calls approveApproval with reason and routes back to /approvals on success", async () => {
    approveApprovalMock.mockResolvedValueOnce({
      kind: "resolved",
      data: {
        gate_id: GATE_ID,
        action_id: "act",
        outcome: "approved",
        resolved_at: 1,
        resolved_by: "user:user_x",
      },
    });
    render(
      React.createElement(ResolveButtons, { workspaceId: WORKSPACE_ID, gateId: GATE_ID }),
    );
    fireEvent.change(screen.getByLabelText(/reason/i), {
      target: { value: "looks good" },
    });
    fireEvent.click(screen.getByRole("button", { name: /^approve$/i }));
    await waitFor(() => {
      expect(approveApprovalMock).toHaveBeenCalledWith(
        WORKSPACE_ID,
        GATE_ID,
        TOKEN,
        "looks good",
      );
      expect(routerPush).toHaveBeenCalledWith("/approvals");
      expect(routerRefresh).toHaveBeenCalled();
    });
  });

  it("omits reason when textarea is empty", async () => {
    approveApprovalMock.mockResolvedValueOnce({
      kind: "resolved",
      data: {
        gate_id: GATE_ID,
        action_id: "act",
        outcome: "approved",
        resolved_at: 1,
        resolved_by: "user:user_x",
      },
    });
    render(
      React.createElement(ResolveButtons, { workspaceId: WORKSPACE_ID, gateId: GATE_ID }),
    );
    fireEvent.click(screen.getByRole("button", { name: /^approve$/i }));
    await waitFor(() => {
      expect(approveApprovalMock).toHaveBeenCalledWith(
        WORKSPACE_ID,
        GATE_ID,
        TOKEN,
        undefined,
      );
    });
  });
});

describe("ResolveButtons — deny happy path", () => {
  it("calls denyApproval and routes back to /approvals on success", async () => {
    denyApprovalMock.mockResolvedValueOnce({
      kind: "resolved",
      data: {
        gate_id: GATE_ID,
        action_id: "act",
        outcome: "denied",
        resolved_at: 1,
        resolved_by: "user:user_x",
      },
    });
    render(
      React.createElement(ResolveButtons, { workspaceId: WORKSPACE_ID, gateId: GATE_ID }),
    );
    fireEvent.click(screen.getByRole("button", { name: /^deny$/i }));
    await waitFor(() => {
      expect(denyApprovalMock).toHaveBeenCalled();
      expect(routerPush).toHaveBeenCalledWith("/approvals");
    });
  });
});

describe("ResolveButtons — 409 already_resolved", () => {
  it("refreshes the page (does NOT push to /approvals) so the terminal state renders", async () => {
    approveApprovalMock.mockResolvedValueOnce({
      kind: "already_resolved",
      data: {
        gate_id: GATE_ID,
        action_id: "act",
        outcome: "approved",
        resolved_at: 1,
        resolved_by: "slack:webhook",
        error_code: ERR_ALREADY_RESOLVED,
        detail: "raced",
      },
    });
    render(
      React.createElement(ResolveButtons, { workspaceId: WORKSPACE_ID, gateId: GATE_ID }),
    );
    fireEvent.click(screen.getByRole("button", { name: /^approve$/i }));
    await waitFor(() => {
      expect(routerRefresh).toHaveBeenCalled();
      expect(routerPush).not.toHaveBeenCalled();
    });
  });
});

describe("ResolveButtons — error paths", () => {
  it("shows alert when not authenticated", async () => {
    getTokenFn.mockResolvedValueOnce(null);
    render(
      React.createElement(ResolveButtons, { workspaceId: WORKSPACE_ID, gateId: GATE_ID }),
    );
    fireEvent.click(screen.getByRole("button", { name: /^approve$/i }));
    await waitFor(() => {
      expect(screen.getByRole("alert").textContent).toMatch(/not authenticated/i);
    });
    expect(approveApprovalMock).not.toHaveBeenCalled();
  });

  it("shows alert when approveApproval rejects (network error)", async () => {
    approveApprovalMock.mockRejectedValueOnce(new Error("ECONNRESET"));
    render(
      React.createElement(ResolveButtons, { workspaceId: WORKSPACE_ID, gateId: GATE_ID }),
    );
    fireEvent.click(screen.getByRole("button", { name: /^approve$/i }));
    await waitFor(() => {
      expect(screen.getByRole("alert").textContent).toMatch(/ECONNRESET/);
    });
  });

  it("falls back to generic 'Resolve failed' when thrown value lacks a message", async () => {
    approveApprovalMock.mockRejectedValueOnce({ unexpected: true });
    render(
      React.createElement(ResolveButtons, { workspaceId: WORKSPACE_ID, gateId: GATE_ID }),
    );
    fireEvent.click(screen.getByRole("button", { name: /^approve$/i }));
    await waitFor(() => {
      expect(screen.getByRole("alert").textContent).toMatch(/Resolve failed/i);
    });
  });
});
