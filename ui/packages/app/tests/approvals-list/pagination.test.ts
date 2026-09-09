import {
  AGENTSFLEET_B,
  AGENT_A_DISPLAY_NAME,
  AGENT_B_DISPLAY_NAME,
  WORKSPACE_ID,
  gate,
  listApprovalsActionMock,
  render,
} from "./harness";
import React from "react";
import { describe, expect, it } from "vitest";
import { screen, waitFor } from "@testing-library/react";
import ApprovalsList from "@/app/(dashboard)/w/[workspaceId]/approvals/components/ApprovalsList";
import { APPROVAL_STATUS, APPROVAL_STATUS_ORDER } from "@/lib/api/approvals-types";

/** One page per status, in the order the component asks for them. */
function pages(byStatus: Partial<Record<string, ReturnType<typeof gate>[]>>) {
  listApprovalsActionMock.mockImplementation((_ws: string, opts: { status?: string }) =>
    Promise.resolve({
      ok: true,
      data: { items: byStatus[opts.status ?? APPROVAL_STATUS.PENDING] ?? [], next_cursor: null },
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

    await waitFor(() =>
      expect(listApprovalsActionMock.mock.calls.length).toBeGreaterThanOrEqual(
        APPROVAL_STATUS_ORDER.length - 1,
      ),
    );
    const asked = new Set<string | undefined>(
      listApprovalsActionMock.mock.calls.map((call) => (call[1] as { status?: string }).status),
    );
    // Every settled status is read on mount; `pending` arrives server-rendered
    // and is re-read by the tick rather than at mount.
    for (const status of APPROVAL_STATUS_ORDER) {
      if (status !== APPROVAL_STATUS.PENDING) expect(asked.has(status)).toBe(true);
    }
  });

  it("keeps the rows it has when one status page fails", async () => {
    const shown = gate({ gate_id: "g1" });
    listApprovalsActionMock.mockImplementation((_ws: string, opts: { status?: string }) =>
      Promise.resolve(
        opts.status === APPROVAL_STATUS.DENIED
          ? { ok: false, error: "upstream exploded", status: 502 }
          : { ok: true, data: { items: [shown], next_cursor: null } },
      ),
    );
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
    listApprovalsActionMock.mockResolvedValue({ ok: false, error: "Not authenticated", status: 401 });
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

    await waitFor(() => expect(listApprovalsActionMock).toHaveBeenCalled());
    for (const call of listApprovalsActionMock.mock.calls) {
      expect((call[1] as { fleetId?: string }).fleetId).toBe("fleet-scope");
    }
  });
});
