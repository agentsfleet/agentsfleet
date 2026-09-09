import {
  AGENTSFLEET_B,
  AGENT_A_DISPLAY_NAME,
  AGENT_B_DISPLAY_NAME,
  WORKSPACE_ID,
  gate,
  listAllApprovalsActionMock,
  render,
  requestedStatuses,
} from "./harness";
import React from "react";
import { describe, expect, it } from "vitest";
import { screen, waitFor } from "@testing-library/react";
import ApprovalsList from "@/app/(dashboard)/w/[workspaceId]/approvals/components/ApprovalsList";
import { APPROVAL_STATUS, APPROVAL_STATUS_ORDER } from "@/lib/api/approvals-types";

/**
 * The rows each status holds, merged the way the server action merges them.
 *
 * One call now, not one per status: the fan-out moved to the server, because
 * Next runs Server Actions one at a time per client and five calls were five
 * sequential round trips.
 */
function pages(byStatus: Partial<Record<string, ReturnType<typeof gate>[]>>) {
  listAllApprovalsActionMock.mockImplementation(
    (_ws: string, _opts: unknown, statuses: readonly string[]) =>
      Promise.resolve({
        ok: true,
        data: {
          items: statuses.flatMap((status) => byStatus[status] ?? []),
          next_cursor: null,
        },
      }),
  );
}

describe("ApprovalsList — one table over every status", () => {
  it("reads one page per status and shows them together", async () => {
    pages({
      [APPROVAL_STATUS.PENDING]: [gate({ gate_id: "g1", created_at: 200 })],
      [APPROVAL_STATUS.APPROVED]: [
        gate({ gate_id: "g2", created_at: 100, fleet_id: AGENTSFLEET_B, status: "approved" }),
      ],
    });
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: [gate({ gate_id: "g1", created_at: 200 })],
        initialCursor: null,
      }),
    );

    // A settled row is the whole point: an approval that vanished when it was
    // given left no record of what had ever been allowed.
    await waitFor(() => expect(screen.getByText(AGENT_B_DISPLAY_NAME)).toBeTruthy());
    expect(screen.getByText(AGENT_A_DISPLAY_NAME)).toBeTruthy();
    // Each row carries its own status, which is what replaced the tabs.
    expect(screen.getAllByText("Pending").length).toBeGreaterThan(0);
    expect(screen.getAllByText("Approved").length).toBeGreaterThan(0);
  });

  it("asks for every status the API can answer", async () => {
    pages({});
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: [],
        initialCursor: null,
      }),
    );

    await waitFor(() => expect(listAllApprovalsActionMock).toHaveBeenCalled());
    // ONE call carrying every settled status, not one call per status.
    expect(listAllApprovalsActionMock).toHaveBeenCalledTimes(1);
    const asked = new Set(requestedStatuses()[0]);
    // `pending` arrives server-rendered; re-reading it here would throw away
    // rows the page was rendered with before the first paint settles.
    for (const status of APPROVAL_STATUS_ORDER) {
      expect(asked.has(status)).toBe(status !== APPROVAL_STATUS.PENDING);
    }
  });

  it("keeps the rows it has when the read fails", async () => {
    const shown = gate({ gate_id: "g1" });
    // The server action is all-or-nothing: one failed page inside it refuses
    // the whole read rather than silently shortening the table.
    listAllApprovalsActionMock.mockResolvedValue({
      ok: false,
      error: "upstream exploded",
      status: 502,
    });
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: [shown],
        initialCursor: null,
      }),
    );

    // A half-read table is worse than a stale one: it silently claims rows are
    // gone. The server-rendered row stays put.
    await waitFor(() => expect(screen.getByText(AGENT_A_DISPLAY_NAME)).toBeTruthy());
  });

  it("names an expired session rather than emptying the table under the operator", async () => {
    listAllApprovalsActionMock.mockResolvedValue({ ok: false, error: "Not authenticated", status: 401 });
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: [gate()],
        initialCursor: null,
      }),
    );

    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toMatch(/session expired/i),
    );
    expect(screen.getByText(AGENT_A_DISPLAY_NAME)).toBeTruthy();
  });

  it("passes the fleet scope to every status read", async () => {
    pages({});
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: [],
        initialCursor: null,
        fleetId: "fleet-scope",
      }),
    );

    await waitFor(() => expect(listAllApprovalsActionMock).toHaveBeenCalled());
    for (const call of listAllApprovalsActionMock.mock.calls) {
      expect((call[1] as { fleetId?: string }).fleetId).toBe("fleet-scope");
    }
    // The scope rides the one call, so it cannot go missing from a status the
    // caller forgot to thread it through.
    expect(listAllApprovalsActionMock).toHaveBeenCalledTimes(1);
  });
});
