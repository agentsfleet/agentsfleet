import React from "react";
import { afterEach, beforeEach, vi } from "vitest";
import { cleanup, fireEvent, render as rtlRender, screen } from "@testing-library/react";
import { TooltipProvider } from "@agentsfleet/design-system";

export const WORKSPACE_ID = "ws_approvals_001";
export const AGENTSFLEET_A = "0195b4ba-8d3a-7f13-8abc-2b3e1e0aa701";
export const AGENTSFLEET_B = "0195b4ba-8d3a-7f13-8abc-2b3e1e0aa702";
export const AGENT_A_DISPLAY_NAME = "Agent Finch-D648";
export const AGENT_B_DISPLAY_NAME = "Agent Finch-DB01";
export const ERR_ALREADY_RESOLVED = "UZ-APPROVAL-006" as const;

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
  // A default so a test that never arranges the read still resolves; per-test
  // cases override it.
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

export function gate(over: Partial<ApprovalGate> = {}): ApprovalGate {
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
    // Settled rows share this table now, so the fixture has to be able to
    // express one: a hardcoded `pending` made every row look unanswered.
    status: over.status ?? "pending",
    detail: over.detail ?? "",
    created_at: over.created_at ?? Date.now() - 60_000,
    timeout_at: over.timeout_at ?? Date.now() + 3_600_000,
    updated_at: over.updated_at ?? null,
    resolved_by: over.resolved_by ?? "",
  };
}

export { listApprovalsActionMock, approveApprovalActionMock, denyApprovalActionMock };

// The dashboard layout mounts exactly one TooltipProvider (`layout.test.tsx`
// pins that, and pins that a bare relative <Time> throws without it). The inbox
// renders relative timestamps, so the suite mounts the provider the same way
// the real page gets one, rather than every call site repeating a wrapper.
export function render(ui: React.ReactElement) {
  return rtlRender(ui, { wrapper: TooltipProvider });
}

// The row's actions are icons whose accessible names carry the request they act
// on ("Approve: mint short-lived credentials for github"), so one row's button
// is distinguishable from another's. Denying is irreversible, so it goes through
// the confirm dialog — these helpers keep every call site reading as the intent
// rather than as two clicks and a regex.
export function approveRow() {
  fireEvent.click(screen.getByRole("button", { name: /^approve:/i }));
}

export function denyRow() {
  fireEvent.click(screen.getByRole("button", { name: /^deny:/i }));
  fireEvent.click(screen.getByRole("button", { name: /^deny$/i }));
}

/** The opts each read asked with, in call order. */
export function requestedOpts(): Record<string, unknown>[] {
  return listApprovalsActionMock.mock.calls.map(
    (call) => (call[1] as Record<string, unknown> | undefined) ?? {},
  );
}
