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

describe("ApprovalsList — reaching older approvals", () => {
  it("offers no control when the server said this is the last page", () => {
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: [gate()],
        initialCursor: null,
      }),
    );
    expect(screen.queryByRole("button", { name: /load older/i })).toBeNull();
  });

  it("walks the cursor and appends the older page", async () => {
    // The page is capped at APPROVALS_PAGE_LIMIT. Without this the inbox simply
    // could not reach a workspace's older gates: the table's own pager just
    // re-divides the rows already fetched.
    listApprovalsActionMock.mockResolvedValue({
      ok: true,
      data: {
        items: [gate({ gate_id: "older-1", action_id: "a-old", fleet_id: AGENTSFLEET_B })],
        next_cursor: null,
      },
    });
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: [gate({ gate_id: "newer-1" })],
        initialCursor: "cur-1",
      }),
    );

    fireEvent.click(screen.getByRole("button", { name: /load older/i }));
    await waitFor(() => expect(screen.getByText(AGENT_B_DISPLAY_NAME)).toBeTruthy());
    // Appended, not replaced: the first page is still on screen.
    expect(screen.getByText(AGENT_A_DISPLAY_NAME)).toBeTruthy();
    expect(requestedOpts()[0]?.cursor).toBe("cur-1");
    // The server said that was the last page, so the control goes.
    await waitFor(() =>
      expect(screen.queryByRole("button", { name: /load older/i })).toBeNull(),
    );
  });

  it("does not show a gate twice when it arrives on both pages", async () => {
    // The keyset resumes strictly past the last row, so a page cannot repeat
    // one — but a gate resolved between the two reads can arrive under its new
    // status, and a duplicate row would be the operator seeing double.
    listApprovalsActionMock.mockResolvedValue({
      ok: true,
      data: { items: [gate({ gate_id: "newer-1" })], next_cursor: null },
    });
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: [gate({ gate_id: "newer-1" })],
        initialCursor: "cur-1",
      }),
    );

    fireEvent.click(screen.getByRole("button", { name: /load older/i }));
    await waitFor(() => expect(listApprovalsActionMock).toHaveBeenCalled());
    await waitFor(() => expect(screen.getAllByText(AGENT_A_DISPLAY_NAME)).toHaveLength(1));
  });

  it("keeps the rows and the cursor when the older page is refused", async () => {
    listApprovalsActionMock.mockResolvedValue({
      ok: false,
      error: "upstream is down",
      status: 503,
    });
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: [gate()],
        initialCursor: "cur-1",
      }),
    );

    fireEvent.click(screen.getByRole("button", { name: /load older/i }));
    await waitFor(() => expect(screen.getByRole("alert").textContent).toMatch(/upstream is down/i));
    expect(screen.getByText(AGENT_A_DISPLAY_NAME)).toBeTruthy();
    // Still offered: a failed page is not the end of the walk. Waited for, not
    // sampled — the control reads "Loading…" until the transition ends.
    await waitFor(() =>
      expect(screen.getByRole("button", { name: /load older/i })).toBeTruthy(),
    );
  });

  it("treats a rejected read as a failed read, not a crash", async () => {
    // The viewer goes offline mid-Refresh and the Server Action REJECTS rather
    // than resolving `ok: false`. Uncaught, that escapes the transition and
    // takes the whole inbox to the error boundary — worse than a stale table.
    listApprovalsActionMock.mockRejectedValue(new Error("Failed to fetch"));
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: [gate()],
        initialCursor: null,
      }),
    );

    fireEvent.click(screen.getByRole("button", { name: /refresh/i }));
    // Surfaced through the same curated copy a refused read gets, and the
    // table is still standing — which is the property. Uncaught, this would
    // have reached the error boundary instead.
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toMatch(/couldn't read the approvals/i),
    );
    expect(screen.getByText(AGENT_A_DISPLAY_NAME)).toBeTruthy();
  });

  it("carries a non-Error rejection through as its own text", async () => {
    listApprovalsActionMock.mockRejectedValue("socket closed");
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: [gate()],
        initialCursor: null,
      }),
    );

    fireEvent.click(screen.getByRole("button", { name: /refresh/i }));
    // A thrown string has no `.message`; `rejectedRead` stringifies it rather
    // than reading `undefined`, so the read still resolves to a failure.
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toMatch(/couldn't read the approvals/i),
    );
    expect(screen.getByText(AGENT_A_DISPLAY_NAME)).toBeTruthy();
  });
});

describe("ApprovalsList — two reads in flight at once", () => {
  /** A promise this test resolves by hand, so the two reads can be interleaved. */
  function deferred<T>() {
    let settle: (value: T) => void = () => {};
    const promise = new Promise<T>((resolve) => {
      settle = resolve;
    });
    return { promise, settle };
  }

  it("drops the older page when a refresh started after it", async () => {
    // The race Greptile named: both controls are independent transitions, so
    // an operator can start "Load older" and then Refresh. If the older page
    // were applied after the refresh, the table would hold rows from two
    // different walks under a cursor matching neither.
    const older = deferred<unknown>();
    const fresh = deferred<unknown>();
    listApprovalsActionMock
      .mockReturnValueOnce(older.promise)
      .mockReturnValueOnce(fresh.promise);

    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: [gate({ gate_id: "newer-1" })],
        initialCursor: "cur-1",
      }),
    );

    fireEvent.click(screen.getByRole("button", { name: /load older/i }));
    clickRefresh();
    await waitFor(() => expect(listApprovalsActionMock).toHaveBeenCalledTimes(2));

    // The refresh was started LAST, so its answer is the one that counts —
    // even though the older page lands first.
    fresh.settle({
      ok: true,
      data: { items: [gate({ gate_id: "refreshed", action_id: "a-r" })], next_cursor: null },
    });
    older.settle({
      ok: true,
      data: {
        items: [gate({ gate_id: "stale", action_id: "a-s", fleet_id: AGENTSFLEET_B })],
        next_cursor: "cur-stale",
      },
    });

    await waitFor(() => expect(screen.getByText(AGENT_A_DISPLAY_NAME)).toBeTruthy());
    // The superseded walk contributed nothing: no row of its own, and no
    // control offering to continue it.
    expect(screen.queryByText(AGENT_B_DISPLAY_NAME)).toBeNull();
    await waitFor(() =>
      expect(screen.queryByRole("button", { name: /load older/i })).toBeNull(),
    );
  });

  it("applies the older page when nothing superseded it", async () => {
    // The same interleaving machinery, with only one read in flight — the
    // guard must not drop a result that is still the newest.
    const older = deferred<unknown>();
    listApprovalsActionMock.mockReturnValueOnce(older.promise);

    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: [gate({ gate_id: "newer-1" })],
        initialCursor: "cur-1",
      }),
    );

    fireEvent.click(screen.getByRole("button", { name: /load older/i }));
    older.settle({
      ok: true,
      data: {
        items: [gate({ gate_id: "older-1", action_id: "a-o", fleet_id: AGENTSFLEET_B })],
        next_cursor: null,
      },
    });

    await waitFor(() => expect(screen.getByText(AGENT_B_DISPLAY_NAME)).toBeTruthy());
    expect(screen.getByText(AGENT_A_DISPLAY_NAME)).toBeTruthy();
  });
});
