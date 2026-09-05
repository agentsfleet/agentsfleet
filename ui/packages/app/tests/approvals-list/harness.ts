import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

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
    status: "pending",
    detail: "",
    created_at: over.created_at ?? Date.now() - 60_000,
    timeout_at: over.timeout_at ?? Date.now() + 3_600_000,
    updated_at: null,
    resolved_by: "",
  };
}

// ── EmptyState ─────────────────────────────────────────────────────────

export { listApprovalsActionMock, approveApprovalActionMock, denyApprovalActionMock };
