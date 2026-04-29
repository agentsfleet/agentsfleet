import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

// ── Constants (RULE UFS) ───────────────────────────────────────────────

const WORKSPACE_ID = "ws_approvals_001";
const ZOMBIE_A = "0195b4ba-8d3a-7f13-8abc-2b3e1e0aa701";
const ZOMBIE_B = "0195b4ba-8d3a-7f13-8abc-2b3e1e0aa702";
const TOKEN = "token_abc";
const ERR_ALREADY_RESOLVED = "UZ-APPROVAL-006" as const;

// ── Shared mocks ───────────────────────────────────────────────────────
// vi.hoisted because vi.mock factories run before module body. The mocks
// must be declared inside the hoisted block so the factory closures can
// reference them without a TDZ error.

const { getTokenFn, listApprovalsMock, approveApprovalMock, denyApprovalMock } =
  vi.hoisted(() => ({
    getTokenFn: vi.fn(),
    listApprovalsMock: vi.fn(),
    approveApprovalMock: vi.fn(),
    denyApprovalMock: vi.fn(),
  }));

vi.mock("@clerk/nextjs", () => ({
  useAuth: () => ({ getToken: getTokenFn }),
}));
vi.mock("@/lib/auth/client", () => ({
  useClientToken: () => ({ getToken: getTokenFn }),
}));
vi.mock("@/lib/api/approvals", () => ({
  listApprovals: listApprovalsMock,
  approveApproval: approveApprovalMock,
  denyApproval: denyApprovalMock,
}));
vi.mock("next/link", () => ({
  default: ({ href, children, ...rest }: { href: string; children: React.ReactNode }) =>
    React.createElement("a", { href, ...rest }, children),
}));

import ApprovalsList from "@/app/(dashboard)/approvals/components/ApprovalsList";
import type { ApprovalGate } from "@/lib/api/approvals";

beforeEach(() => {
  // Default the polling mock so the 5s setInterval fallback path never sees
  // an undefined resolved value. Per-test cases override with mockResolvedValueOnce.
  listApprovalsMock.mockResolvedValue({ items: [], next_cursor: null });
  getTokenFn.mockResolvedValue(TOKEN);
});

afterEach(() => {
  cleanup();
  listApprovalsMock.mockReset();
  approveApprovalMock.mockReset();
  denyApprovalMock.mockReset();
  getTokenFn.mockReset();
});

function gate(over: Partial<ApprovalGate> = {}): ApprovalGate {
  return {
    gate_id: over.gate_id ?? "01999999-0000-7000-8000-000000000001",
    zombie_id: over.zombie_id ?? ZOMBIE_A,
    zombie_name: over.zombie_name ?? "approvals-a",
    workspace_id: WORKSPACE_ID,
    action_id: over.action_id ?? "act_001",
    tool_name: over.tool_name ?? "write_repo",
    action_name: over.action_name ?? "create_pr",
    gate_kind: over.gate_kind ?? "destructive_action",
    proposed_action: over.proposed_action ?? "Open PR titled X",
    evidence: over.evidence ?? {},
    blast_radius: over.blast_radius ?? "single repo branch",
    status: "pending",
    detail: "",
    requested_at: over.requested_at ?? Date.now() - 60_000,
    timeout_at: over.timeout_at ?? Date.now() + 3_600_000,
    updated_at: null,
    resolved_by: "",
  };
}

// ── EmptyState ─────────────────────────────────────────────────────────

describe("ApprovalsList — EmptyState", () => {
  it("renders the EmptyState when there are no items and no filter", () => {
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: [],
        initialCursor: null,
      }),
    );
    expect(screen.getByText(/no pending approvals/i)).toBeTruthy();
  });
});

// ── Initial render with items ─────────────────────────────────────────

describe("ApprovalsList — initial render", () => {
  it("renders zombie name, gate kind badge, and approve/deny buttons per row", () => {
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: [gate()],
        initialCursor: null,
      }),
    );
    expect(screen.getByText("approvals-a")).toBeTruthy();
    expect(screen.getByText("destructive_action")).toBeTruthy();
    expect(screen.getByRole("button", { name: /^approve$/i })).toBeTruthy();
    expect(screen.getByRole("button", { name: /^deny$/i })).toBeTruthy();
    expect(screen.getByRole("link", { name: /details/i })).toBeTruthy();
  });

  it("renders one card per item", () => {
    const items = [
      gate({ gate_id: "01999999-0000-7000-8000-000000000001", action_id: "a1" }),
      gate({
        gate_id: "01999999-0000-7000-8000-000000000002",
        action_id: "a2",
        zombie_name: "approvals-b",
        zombie_id: ZOMBIE_B,
      }),
    ];
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: items,
        initialCursor: null,
      }),
    );
    expect(screen.getByText("approvals-a")).toBeTruthy();
    expect(screen.getByText("approvals-b")).toBeTruthy();
  });
});

// ── Filter input ──────────────────────────────────────────────────────

describe("ApprovalsList — client-side filter", () => {
  it("hides rows that don't match the filter input", () => {
    const items = [
      gate({ gate_id: "01999999-1111-7000-8000-000000000001", proposed_action: "Open PR titled wire" }),
      gate({
        gate_id: "01999999-1111-7000-8000-000000000002",
        proposed_action: "Drop production database",
        zombie_name: "approvals-b",
        action_id: "a2",
      }),
    ];
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: items,
        initialCursor: null,
      }),
    );
    fireEvent.change(screen.getByLabelText(/filter approvals/i), {
      target: { value: "wire" },
    });
    expect(screen.getByText(/Open PR titled wire/i)).toBeTruthy();
    expect(screen.queryByText(/Drop production database/i)).toBeNull();
  });
});

// ── Approve / Deny — optimistic resolve ───────────────────────────────

describe("ApprovalsList — resolve actions", () => {
  it("optimistically removes a row when approveApproval returns kind=resolved", async () => {
    approveApprovalMock.mockResolvedValueOnce({
      kind: "resolved",
      data: {
        gate_id: "01999999-0000-7000-8000-000000000001",
        action_id: "act_001",
        outcome: "approved",
        resolved_at: Date.now(),
        resolved_by: "user:user_abc",
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
      expect(approveApprovalMock).toHaveBeenCalledWith(
        WORKSPACE_ID,
        "01999999-0000-7000-8000-000000000001",
        TOKEN,
      );
      // Row removed → EmptyState renders
      expect(screen.queryByText("approvals-a")).toBeNull();
    });
  });

  it("optimistically removes a row when denyApproval returns kind=resolved", async () => {
    denyApprovalMock.mockResolvedValueOnce({
      kind: "resolved",
      data: {
        gate_id: "01999999-0000-7000-8000-000000000001",
        action_id: "act_001",
        outcome: "denied",
        resolved_at: Date.now(),
        resolved_by: "user:user_abc",
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
      expect(denyApprovalMock).toHaveBeenCalled();
      expect(screen.queryByText("approvals-a")).toBeNull();
    });
  });

  it("surfaces an alert when 409 already_resolved comes back", async () => {
    approveApprovalMock.mockResolvedValueOnce({
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
  });

  it("shows an error when not authenticated (getToken returns null)", async () => {
    getTokenFn.mockResolvedValueOnce(null);
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
    expect(approveApprovalMock).not.toHaveBeenCalled();
  });

  it("renders error message when approveApproval rejects", async () => {
    approveApprovalMock.mockRejectedValueOnce(new Error("ECONNRESET"));
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

  it("falls back to generic 'Resolve failed' when thrown value lacks a message", async () => {
    // Thrown value has no `.message` property — exercises the `?? "Resolve failed"` branch.
    approveApprovalMock.mockRejectedValueOnce({ not: "an error" });
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: [gate()],
        initialCursor: null,
      }),
    );
    fireEvent.click(screen.getByRole("button", { name: /^approve$/i }));
    await waitFor(() => {
      expect(screen.getByRole("alert").textContent).toMatch(/Resolve failed/i);
    });
  });
});

// ── Pagination ────────────────────────────────────────────────────────

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
    listApprovalsMock.mockResolvedValueOnce({
      items: [
        gate({
          gate_id: "01999999-0000-7000-8000-000000000099",
          action_id: "act_099",
          zombie_name: "approvals-c",
        }),
      ],
      next_cursor: null,
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
      expect(listApprovalsMock).toHaveBeenCalledWith(
        WORKSPACE_ID,
        TOKEN,
        expect.objectContaining({ cursor: "cur_abc", limit: 50 }),
      );
      expect(screen.getByText("approvals-c")).toBeTruthy();
      expect(screen.queryByRole("button", { name: /load more/i })).toBeNull();
    });
  });
});

// ── zombie_id scoping ────────────────────────────────────────────────

describe("ApprovalsList — zombieId scoping", () => {
  it("passes zombieId to listApprovals on Load more", async () => {
    listApprovalsMock.mockResolvedValueOnce({ items: [], next_cursor: null });
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        zombieId: ZOMBIE_A,
        initialItems: [gate()],
        initialCursor: "cur_abc",
      }),
    );
    fireEvent.click(screen.getByRole("button", { name: /load more/i }));
    await waitFor(() => {
      expect(listApprovalsMock).toHaveBeenCalledWith(
        WORKSPACE_ID,
        TOKEN,
        expect.objectContaining({ zombieId: ZOMBIE_A }),
      );
    });
  });
});

// ── Branch coverage edges ────────────────────────────────────────────

describe("ApprovalsList — branch coverage", () => {
  it("Load more shows Not authenticated when getToken returns null", async () => {
    getTokenFn.mockResolvedValueOnce(null);
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

  it("Load more surfaces error when listApprovals rejects", async () => {
    listApprovalsMock.mockRejectedValueOnce(new Error("upstream 503"));
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
    denyApprovalMock.mockResolvedValueOnce({
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
    approveApprovalMock.mockRejectedValueOnce(new Error("ECONNRESET"));
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: [gate()],
        initialCursor: null,
      }),
    );
    fireEvent.click(screen.getByRole("button", { name: /^approve$/i }));
    // Optimistic remove → 0 items + no filter, but error is set, so alert shows
    // and EmptyState does NOT render.
    await waitFor(() => {
      expect(screen.getByRole("alert")).toBeTruthy();
      expect(screen.queryByText(/no pending approvals/i)).toBeNull();
    });
  });
});

// ── Polling effect ────────────────────────────────────────────────────

describe("ApprovalsList — 5s polling effect", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("refreshes items + cursor on each polling tick", async () => {
    listApprovalsMock.mockResolvedValueOnce({
      items: [gate({ gate_id: "01999999-aaaa-7000-8000-000000000001", action_id: "polled" })],
      next_cursor: "cur_polled",
    });
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: [gate({ action_id: "initial" })],
        initialCursor: null,
      }),
    );
    // Advance past the 5s POLL_MS interval. advanceTimersByTimeAsync flushes
    // both fake timers and the awaited microtasks the poll callback enqueues.
    await vi.advanceTimersByTimeAsync(5_001);
    expect(listApprovalsMock).toHaveBeenCalledWith(
      WORKSPACE_ID,
      TOKEN,
      expect.objectContaining({ limit: 50 }),
    );
  });

  it("polling skips the update when getToken returns null", async () => {
    getTokenFn.mockResolvedValueOnce(null);
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: [gate()],
        initialCursor: null,
      }),
    );
    await vi.advanceTimersByTimeAsync(5_001);
    expect(listApprovalsMock).not.toHaveBeenCalled();
  });

  it("polling absorbs upstream errors silently (list stays as-is)", async () => {
    listApprovalsMock.mockRejectedValueOnce(new Error("transient 503"));
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: [gate()],
        initialCursor: null,
      }),
    );
    await vi.advanceTimersByTimeAsync(5_001);
    // No alert from polling errors — the spec says "Transient — leave the
    // existing list rendered until the next tick."
    expect(screen.queryByRole("alert")).toBeNull();
    expect(screen.getByText("approvals-a")).toBeTruthy();
  });

  it("polling skips the reset once the operator has clicked Load more", async () => {
    // Initial page-2 load via the cursor-bearing initial state.
    listApprovalsMock.mockResolvedValueOnce({
      items: [
        gate({
          gate_id: "01999999-bbbb-7000-8000-000000000099",
          action_id: "appended",
          zombie_name: "approvals-c",
        }),
      ],
      next_cursor: null,
    });
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: [gate({ action_id: "page-1-row" })],
        initialCursor: "cur_abc",
      }),
    );
    // Click Load more — extends the visible list past page 1 and latches
    // the polling guard. Advance just enough to flush the load-more
    // microtask without firing the polling setInterval (POLL_MS = 5000).
    fireEvent.click(screen.getByRole("button", { name: /load more/i }));
    await vi.advanceTimersByTimeAsync(50);
    // Poll the mock that the next interval would normally pull from.
    listApprovalsMock.mockResolvedValueOnce({
      items: [
        gate({ gate_id: "01999999-cccc-7000-8000-000000000001", action_id: "fresh-page-1" }),
      ],
      next_cursor: null,
    });
    await vi.advanceTimersByTimeAsync(5_001);
    // The poll did NOT reset items — the appended page-2 row is still there
    // and the page-1 row hasn't been replaced by "fresh-page-1".
    expect(screen.getByText("approvals-c")).toBeTruthy();
    expect(screen.queryByText("fresh-page-1")).toBeNull();
  });
});
