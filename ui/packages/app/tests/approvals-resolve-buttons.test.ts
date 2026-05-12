import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

const WORKSPACE_ID = "ws_resolve_001";
const GATE_ID = "01999999-0000-7000-8000-000000000001";
const ERR_ALREADY_RESOLVED = "UZ-APPROVAL-006" as const;

const { approveActionMock, denyActionMock, routerPush, routerRefresh } =
  vi.hoisted(() => ({
    approveActionMock: vi.fn(),
    denyActionMock: vi.fn(),
    routerPush: vi.fn(),
    routerRefresh: vi.fn(),
  }));

vi.mock("next/navigation", () => ({
  useRouter: () => ({ push: routerPush, refresh: routerRefresh }),
}));
vi.mock("@/app/(dashboard)/approvals/actions", () => ({
  approveApprovalAction: approveActionMock,
  denyApprovalAction: denyActionMock,
}));

import ResolveButtons from "@/app/(dashboard)/approvals/[gateId]/ResolveButtons";

beforeEach(() => {
  approveActionMock.mockReset();
  denyActionMock.mockReset();
  routerPush.mockReset();
  routerRefresh.mockReset();
});

afterEach(() => cleanup());

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
  it("calls approveApprovalAction with reason and routes back to /approvals on success", async () => {
    approveActionMock.mockResolvedValueOnce({
      ok: true,
      data: {
        kind: "resolved",
        data: {
          gate_id: GATE_ID,
          action_id: "act",
          outcome: "approved",
          resolved_at: 1,
          resolved_by: "user:user_x",
        },
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
      expect(approveActionMock).toHaveBeenCalledWith(WORKSPACE_ID, GATE_ID, "looks good");
      expect(routerPush).toHaveBeenCalledWith("/approvals");
      expect(routerRefresh).toHaveBeenCalled();
    });
  });

  it("omits reason when textarea is empty", async () => {
    approveActionMock.mockResolvedValueOnce({
      ok: true,
      data: {
        kind: "resolved",
        data: {
          gate_id: GATE_ID,
          action_id: "act",
          outcome: "approved",
          resolved_at: 1,
          resolved_by: "user:user_x",
        },
      },
    });
    render(
      React.createElement(ResolveButtons, { workspaceId: WORKSPACE_ID, gateId: GATE_ID }),
    );
    fireEvent.click(screen.getByRole("button", { name: /^approve$/i }));
    await waitFor(() => {
      expect(approveActionMock).toHaveBeenCalledWith(WORKSPACE_ID, GATE_ID, undefined);
    });
  });
});

describe("ResolveButtons — deny happy path", () => {
  it("calls denyApprovalAction and routes back to /approvals on success", async () => {
    denyActionMock.mockResolvedValueOnce({
      ok: true,
      data: {
        kind: "resolved",
        data: {
          gate_id: GATE_ID,
          action_id: "act",
          outcome: "denied",
          resolved_at: 1,
          resolved_by: "user:user_x",
        },
      },
    });
    render(
      React.createElement(ResolveButtons, { workspaceId: WORKSPACE_ID, gateId: GATE_ID }),
    );
    fireEvent.click(screen.getByRole("button", { name: /^deny$/i }));
    await waitFor(() => {
      expect(denyActionMock).toHaveBeenCalled();
      expect(routerPush).toHaveBeenCalledWith("/approvals");
    });
  });
});

describe("ResolveButtons — 409 already_resolved", () => {
  it("refreshes the page (does NOT push to /approvals) so the terminal state renders", async () => {
    approveActionMock.mockResolvedValueOnce({
      ok: true,
      data: {
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
  it("shows alert when the server action reports unauth", async () => {
    approveActionMock.mockResolvedValueOnce({ ok: false, error: "Not authenticated", status: 401 });
    render(
      React.createElement(ResolveButtons, { workspaceId: WORKSPACE_ID, gateId: GATE_ID }),
    );
    fireEvent.click(screen.getByRole("button", { name: /^approve$/i }));
    await waitFor(() => {
      expect(screen.getByRole("alert").textContent).toMatch(/not authenticated/i);
    });
  });

  it("shows alert when approve action returns a network-level error", async () => {
    approveActionMock.mockResolvedValueOnce({ ok: false, error: "ECONNRESET" });
    render(
      React.createElement(ResolveButtons, { workspaceId: WORKSPACE_ID, gateId: GATE_ID }),
    );
    fireEvent.click(screen.getByRole("button", { name: /^approve$/i }));
    await waitFor(() => {
      expect(screen.getByRole("alert").textContent).toMatch(/ECONNRESET/);
    });
  });

  it("falls back to presentError default when action returns an empty error", async () => {
    approveActionMock.mockResolvedValueOnce({ ok: false, error: "" });
    render(
      React.createElement(ResolveButtons, { workspaceId: WORKSPACE_ID, gateId: GATE_ID }),
    );
    fireEvent.click(screen.getByRole("button", { name: /^approve$/i }));
    // WS-G — empty server error falls through presentError's default path.
    await waitFor(() => {
      expect(screen.getByRole("alert").textContent).toMatch(/Couldn't approve this request/i);
    });
  });
});
