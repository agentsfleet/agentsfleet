import {
  AGENTSFLEET_B,
  AGENT_A_DISPLAY_NAME,
  AGENT_B_DISPLAY_NAME,
  WORKSPACE_ID,
  gate,
  listApprovalsActionMock,
  render,
  requestedOpts,
} from "./harness";
import React from "react";
import { describe, expect, it } from "vitest";
import { fireEvent, screen, waitFor } from "@testing-library/react";
import ApprovalsList from "@/app/(dashboard)/w/[workspaceId]/approvals/components/ApprovalsList";
import { APPROVALS_PAGE_LIMIT } from "@/lib/api/approvals-types";

/** Clicks the section's manual re-read. */
function clickRefresh() {
  fireEvent.click(screen.getByRole("button", { name: /refresh/i }));
}

/** Answers the one read with these rows. */
function answersWith(items: ReturnType<typeof gate>[]) {
  listApprovalsActionMock.mockResolvedValue({
    ok: true,
    data: { items, next_cursor: null },
  });
}

describe("ApprovalsList — one read, and only when asked", () => {
  it("shows every state from the server render, having read nothing", () => {
    // The page hands over pending AND settled rows in one query. This is the
    // shape that used to cost five sequential Server Actions and 4.6s.
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: [
          gate({ gate_id: "g1", created_at: 200 }),
          gate({
            gate_id: "g2",
            action_id: "a2",
            created_at: 100,
            fleet_id: AGENTSFLEET_B,
            status: "approved",
          }),
        ],
        initialCursor: null,
      }),
    );

    expect(screen.getByText(AGENT_A_DISPLAY_NAME)).toBeTruthy();
    // A settled row is the whole point: an approval that vanished when it was
    // given left no record of what had ever been allowed.
    expect(screen.getByText(AGENT_B_DISPLAY_NAME)).toBeTruthy();
    // Each row carries its own status, which is what replaced the tabs.
    expect(screen.getAllByText("Pending").length).toBeGreaterThan(0);
    expect(screen.getAllByText("Approved").length).toBeGreaterThan(0);
    expect(listApprovalsActionMock).not.toHaveBeenCalled();
  });

  it("refreshes with exactly one call that names no status", async () => {
    answersWith([]);
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: [],
        initialCursor: null,
      }),
    );

    clickRefresh();
    await waitFor(() => expect(listApprovalsActionMock).toHaveBeenCalled());
    expect(listApprovalsActionMock).toHaveBeenCalledTimes(1);
    // Omitting `status` is the filter being OFF. Naming one would narrow the
    // table back to a single state and put the other four behind more reads.
    const [opts] = requestedOpts();
    expect(opts).not.toHaveProperty("status");
    expect(opts).toEqual({ limit: APPROVALS_PAGE_LIMIT, fleetId: undefined });
  });

  it("replaces the table with what the refresh returned", async () => {
    answersWith([
      gate({ gate_id: "g9", action_id: "a9", fleet_id: AGENTSFLEET_B, status: "denied" }),
    ]);
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: [gate({ gate_id: "g1" })],
        initialCursor: null,
      }),
    );

    clickRefresh();
    await waitFor(() => expect(screen.getByText(AGENT_B_DISPLAY_NAME)).toBeTruthy());
    expect(screen.queryByText(AGENT_A_DISPLAY_NAME)).toBeNull();
  });

  it("keeps the rows it has when the read fails", async () => {
    listApprovalsActionMock.mockResolvedValue({
      ok: false,
      error: "upstream exploded",
      status: 502,
    });
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: [gate({ gate_id: "g1" })],
        initialCursor: null,
      }),
    );

    clickRefresh();
    // A half-read table is worse than a stale one: it silently claims rows are
    // gone. The server-rendered row stays put and the alert says why.
    await waitFor(() => expect(screen.getByRole("alert").textContent).toMatch(/upstream exploded/i));
    expect(screen.getByText(AGENT_A_DISPLAY_NAME)).toBeTruthy();
  });

  it("names an expired session rather than emptying the table under the operator", async () => {
    listApprovalsActionMock.mockResolvedValue({
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

    clickRefresh();
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toMatch(/session expired/i),
    );
    expect(screen.getByText(AGENT_A_DISPLAY_NAME)).toBeTruthy();
  });

  it("passes the fleet scope to the read", async () => {
    answersWith([]);
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: [],
        initialCursor: null,
        fleetId: "fleet-scope",
      }),
    );

    clickRefresh();
    await waitFor(() => expect(listApprovalsActionMock).toHaveBeenCalled());
    // The scope rides the one call, so it cannot go missing from a status the
    // caller forgot to thread it through — there are no other statuses now.
    expect(listApprovalsActionMock).toHaveBeenCalledTimes(1);
    expect(requestedOpts()[0]?.fleetId).toBe("fleet-scope");
  });
});
