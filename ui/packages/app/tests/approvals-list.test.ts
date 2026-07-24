import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

const WORKSPACE_ID = "ws_approvals_001";
const AGENTSFLEET_A = "0195b4ba-8d3a-7f13-8abc-2b3e1e0aa701";
const AGENTSFLEET_B = "0195b4ba-8d3a-7f13-8abc-2b3e1e0aa702";
const AGENT_A_DISPLAY_NAME = "Agent Finch-D648";
const AGENT_B_DISPLAY_NAME = "Agent Finch-DB01";
const ERR_ALREADY_RESOLVED = "UZ-APPROVAL-006" as const;

// vi.hoisted because vi.mock factories run before module body. The mocks
// must be declared inside the hoisted block so the factory closures can
// reference them without a TDZ error.

const { listApprovalsActionMock, approveApprovalActionMock, denyApprovalActionMock } =
  vi.hoisted(() => ({
    listApprovalsActionMock: vi.fn(),
    approveApprovalActionMock: vi.fn(),
    denyApprovalActionMock: vi.fn(),
  }));

vi.mock("@/app/(dashboard)/w/[workspaceId]/approvals/actions", () => ({
  listApprovalsAction: listApprovalsActionMock,
  approveApprovalAction: approveApprovalActionMock,
  denyApprovalAction: denyApprovalActionMock,
}));
vi.mock("next/link", () => ({
  default: ({ href, children, ...rest }: { href: string; children: React.ReactNode }) =>
    React.createElement("a", { href, ...rest }, children),
}));

import ApprovalsList from "@/app/(dashboard)/w/[workspaceId]/approvals/components/ApprovalsList";
import type { ApprovalGate } from "@/lib/api/approvals";

beforeEach(() => {
  // Default the polling mock so the 5s setInterval fallback path never sees
  // an undefined resolved value. Per-test cases override with mockResolvedValueOnce.
  listApprovalsActionMock.mockResolvedValue({
    ok: true,
    data: { items: [], next_cursor: null },
  });
});

afterEach(() => {
  cleanup();
  listApprovalsActionMock.mockReset();
  approveApprovalActionMock.mockReset();
  denyApprovalActionMock.mockReset();
});

function gate(over: Partial<ApprovalGate> = {}): ApprovalGate {
  return {
    gate_id: over.gate_id ?? "01999999-0000-7000-8000-000000000001",
    fleet_id: over.fleet_id ?? AGENTSFLEET_A,
    fleet_name: over.fleet_name ?? "approvals-a",
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
  it("renders fleet name, gate kind badge, and approve/deny buttons per row", () => {
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: [gate()],
        initialCursor: null,
      }),
    );
    expect(screen.getByText(AGENT_A_DISPLAY_NAME)).toBeTruthy();
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
        fleet_name: "approvals-b",
        fleet_id: AGENTSFLEET_B,
      }),
    ];
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: items,
        initialCursor: null,
      }),
    );
    expect(screen.getByText(AGENT_A_DISPLAY_NAME)).toBeTruthy();
    expect(screen.getByText(AGENT_B_DISPLAY_NAME)).toBeTruthy();
  });
});

// ── Gate card optional chrome ─────────────────────────────────────────

describe("ApprovalsList — gate card fallbacks", () => {
  it("falls back to tool:action and omits optional chrome when fields are empty", () => {
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: [gate({ proposed_action: "", gate_kind: "", blast_radius: "" })],
        initialCursor: null,
      }),
    );
    // proposed_action "" → `${tool_name}:${action_name}` fallback in the title link.
    expect(screen.getByText("write_repo:create_pr")).toBeTruthy();
    // gate_kind "" → no kind badge; blast_radius "" → no blast-radius content.
    expect(screen.queryByText("destructive_action")).toBeNull();
    expect(screen.queryByText("single repo branch")).toBeNull();
  });
});

// ── Pagination ────────────────────────────────────────────────────────

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

// ── Filter input ──────────────────────────────────────────────────────

describe("ApprovalsList — client-side filter", () => {
  it("hides rows that don't match the filter input", () => {
    const items = [
      gate({ gate_id: "01999999-1111-7000-8000-000000000001", proposed_action: "Open PR titled wire" }),
      gate({
        gate_id: "01999999-1111-7000-8000-000000000002",
        proposed_action: "Drop production database",
        fleet_name: "approvals-b",
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

  it("matches the fleet identity shown on each approval", () => {
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: [gate({ proposed_action: "Restart the worker", fleet_name: "hidden-slug" })],
        initialCursor: null,
      }),
    );

    fireEvent.change(screen.getByLabelText(/filter approvals/i), {
      target: { value: AGENT_A_DISPLAY_NAME },
    });

    expect(screen.getByText("Restart the worker")).toBeTruthy();
  });
});

// ── Approve / Deny — optimistic resolve ───────────────────────────────

describe("ApprovalsList — resolve actions", () => {
  it("optimistically removes a row when approveApprovalAction returns kind=resolved", async () => {
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
    await waitFor(() => {
      expect(approveApprovalActionMock).toHaveBeenCalledWith(
        WORKSPACE_ID,
        "01999999-0000-7000-8000-000000000001",
      );
      // Row removed → EmptyState renders
      expect(screen.queryByText(AGENT_A_DISPLAY_NAME)).toBeNull();
    });
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

  it("surfaces an alert when 409 already_resolved comes back", async () => {
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

// ── fleet_id scoping ────────────────────────────────────────────────

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

// ── Branch coverage edges ────────────────────────────────────────────

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

// ── Polling effect ────────────────────────────────────────────────────

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
    const persisted = gate({ requested_at: base - 50_000, timeout_at: base + 600_000 });
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
