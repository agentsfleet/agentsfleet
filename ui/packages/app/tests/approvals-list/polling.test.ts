import { AGENT_A_DISPLAY_NAME, WORKSPACE_ID, gate, listApprovalsActionMock } from "./harness";
import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import ApprovalsList from "@/app/(dashboard)/w/[workspaceId]/approvals/components/ApprovalsList";

describe("ApprovalsList — 5s polling effect", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("refreshes items + cursor on each polling tick", async () => {
    listApprovalsActionMock.mockResolvedValueOnce({
      ok: true,
      data: {
        items: [gate({ gate_id: "01999999-aaaa-7000-8000-000000000001", action_id: "polled" })],
        next_cursor: "cur_polled",
      },
    });
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: [gate({ action_id: "initial" })],
        initialCursor: null,
      }),
    );
    await vi.advanceTimersByTimeAsync(5_001);
    expect(listApprovalsActionMock).toHaveBeenCalledWith(
      WORKSPACE_ID,
      expect.objectContaining({ limit: 50 }),
    );
  });

  it("refreshes the ApprovalCard relative-age label on its 30s timer", async () => {
    const base = Date.UTC(2026, 4, 15, 18, 0, 0);
    vi.setSystemTime(base);
    const persisted = gate({ created_at: base - 50_000, timeout_at: base + 600_000 });
    // Polls re-fetch the same row so the card (and its `now` state) persists
    // across the 30s window; the 30s interval re-reads Date.now() into `now`.
    listApprovalsActionMock.mockResolvedValue({
      ok: true,
      data: { items: [persisted], next_cursor: null },
    });
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: [persisted],
        initialCursor: null,
      }),
    );
    expect(screen.getByText(/requested 0m ago/)).toBeTruthy();
    await vi.advanceTimersByTimeAsync(30_001);
    expect(screen.getByText(/requested 1m ago/)).toBeTruthy();
  });

  it("polling skips the update when the action reports unauth", async () => {
    listApprovalsActionMock.mockResolvedValueOnce({
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
    await vi.advanceTimersByTimeAsync(5_001);
    // Polling absorbs the failure silently — no alert, the existing row stays.
    expect(screen.queryByRole("alert")).toBeNull();
    expect(screen.getByText(AGENT_A_DISPLAY_NAME)).toBeTruthy();
  });

  it("polling absorbs upstream errors silently (list stays as-is)", async () => {
    listApprovalsActionMock.mockResolvedValueOnce({ ok: false, error: "transient 503" });
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: [gate()],
        initialCursor: null,
      }),
    );
    await vi.advanceTimersByTimeAsync(5_001);
    expect(screen.queryByRole("alert")).toBeNull();
    expect(screen.getByText(AGENT_A_DISPLAY_NAME)).toBeTruthy();
  });

  it("a tick still in flight is not stacked by the next", async () => {
    // The first read never answers; the interval keeps firing. One read on the
    // wire is the ceiling, however slow the backend is.
    listApprovalsActionMock.mockReturnValueOnce(new Promise(() => {}));
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: [gate()],
        initialCursor: null,
      }),
    );
    await vi.advanceTimersByTimeAsync(15_001); // three ticks
    expect(listApprovalsActionMock).toHaveBeenCalledTimes(1);
  });

  it("a hidden tab asks for nothing, and one read catches it up when it is looked at again", async () => {
    const visibility = vi.spyOn(document, "visibilityState", "get").mockReturnValue("hidden");
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: [gate()],
        initialCursor: null,
      }),
    );
    await vi.advanceTimersByTimeAsync(15_001); // three ticks, all skipped
    expect(listApprovalsActionMock).not.toHaveBeenCalled();
    // A visibility change that lands with the tab still hidden asks nothing.
    document.dispatchEvent(new Event("visibilitychange"));
    await vi.advanceTimersByTimeAsync(0);
    expect(listApprovalsActionMock).not.toHaveBeenCalled();

    visibility.mockReturnValue("visible");
    document.dispatchEvent(new Event("visibilitychange"));
    await vi.advanceTimersByTimeAsync(0);
    expect(listApprovalsActionMock).toHaveBeenCalledTimes(1);
    visibility.mockRestore();
  });

  it("polling no-ops if the component unmounts before the request resolves", async () => {
    let release: (v: unknown) => void = () => {};
    listApprovalsActionMock.mockReturnValueOnce(
      new Promise((r) => {
        release = r;
      }),
    );
    const { unmount } = render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: [gate()],
        initialCursor: null,
      }),
    );
    await vi.advanceTimersByTimeAsync(5_001); // fire the tick → it awaits the pending action
    unmount(); // effect cleanup sets `alive = false` and clears the interval
    // The request resolves AFTER unmount; the resumed tick must hit the
    // `!alive` guard and skip the state update (no act warning, no throw).
    release({ ok: true, data: { items: [gate({ action_id: "late" })], next_cursor: "late" } });
    await vi.advanceTimersByTimeAsync(0);
    expect(listApprovalsActionMock).toHaveBeenCalledTimes(1);
  });

  it("polling skips the reset once the operator has clicked Load more", async () => {
    // Initial page-2 load via the cursor-bearing initial state.
    listApprovalsActionMock.mockResolvedValueOnce({
      ok: true,
      data: {
        items: [
          gate({
            gate_id: "01999999-bbbb-7000-8000-000000000099",
            action_id: "appended",
            fleet_name: "approvals-c",
          }),
        ],
        next_cursor: null,
      },
    });
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: [gate({ action_id: "page-1-row" })],
        initialCursor: "cur_abc",
      }),
    );
    fireEvent.click(screen.getByRole("button", { name: /load more/i }));
    await vi.advanceTimersByTimeAsync(50);
    listApprovalsActionMock.mockResolvedValueOnce({
      ok: true,
      data: {
        items: [
          gate({ gate_id: "01999999-cccc-7000-8000-000000000001", action_id: "fresh-page-1" }),
        ],
        next_cursor: null,
      },
    });
    await vi.advanceTimersByTimeAsync(5_001);
    expect(screen.getAllByText(AGENT_A_DISPLAY_NAME)).toHaveLength(2);
    expect(screen.queryByText("fresh-page-1")).toBeNull();
  });
});
