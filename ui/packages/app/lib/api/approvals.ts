import { request } from "./client";

// Mirrors the server's PendingRow envelope (src/zombie/approval_gate_db_reads.zig)
// verbatim — no shim, no rename. Renders the same shape the dashboard queries.

export type ApprovalStatus = "pending" | "approved" | "denied" | "timed_out" | "auto_killed";

export type ApprovalGate = {
  gate_id: string;
  zombie_id: string;
  zombie_name: string;
  workspace_id: string;
  action_id: string;
  tool_name: string;
  action_name: string;
  gate_kind: string;
  proposed_action: string;
  evidence: Record<string, unknown>;
  blast_radius: string;
  status: ApprovalStatus | string;
  detail: string;
  /** epoch ms */
  requested_at: number;
  /** epoch ms — sweeper auto-denies after this */
  timeout_at: number;
  /** epoch ms; null when still pending */
  updated_at: number | null;
  resolved_by: string;
};

export type ApprovalsListResponse = {
  items: ApprovalGate[];
  next_cursor: string | null;
};

export type ResolveResponse = {
  gate_id: string;
  action_id: string;
  outcome: ApprovalStatus;
  resolved_at: number;
  resolved_by: string;
};

export type AlreadyResolvedResponse = ResolveResponse & {
  error_code: "UZ-APPROVAL-006";
  detail: string;
};

export type ResolveOutcome =
  | { kind: "resolved"; data: ResolveResponse }
  | { kind: "already_resolved"; data: AlreadyResolvedResponse };

export type ListApprovalsOpts = {
  status?: string;
  zombieId?: string;
  gateKind?: string;
  cursor?: string;
  limit?: number;
};

export async function listApprovals(
  workspaceId: string,
  token: string,
  opts: ListApprovalsOpts = {},
): Promise<ApprovalsListResponse> {
  const params = new URLSearchParams();
  if (opts.status) params.set("status", opts.status);
  if (opts.zombieId) params.set("zombie_id", opts.zombieId);
  if (opts.gateKind) params.set("gate_kind", opts.gateKind);
  if (opts.cursor) params.set("cursor", opts.cursor);
  if (opts.limit != null) params.set("limit", String(opts.limit));
  const qs = params.toString();
  const path = qs
    ? `/v1/workspaces/${workspaceId}/approvals?${qs}`
    : `/v1/workspaces/${workspaceId}/approvals`;
  return request<ApprovalsListResponse>(path, { method: "GET" }, token);
}

export async function getApproval(
  workspaceId: string,
  gateId: string,
  token: string,
): Promise<ApprovalGate> {
  return request<ApprovalGate>(
    `/v1/workspaces/${workspaceId}/approvals/${gateId}`,
    { method: "GET" },
    token,
  );
}

// Resolve. The server returns 200 with ResolveResponse on success and 409 with
// AlreadyResolvedResponse when another channel got there first. Both are
// expected outcomes from the operator's perspective — we surface them to the
// caller as a tagged union instead of throwing on 409.
async function resolveAction(
  workspaceId: string,
  gateId: string,
  decision: "approve" | "deny",
  token: string,
  reason?: string,
): Promise<ResolveOutcome> {
  const body = JSON.stringify(reason ? { reason } : {});
  const url = `/v1/workspaces/${workspaceId}/approvals/${gateId}:${decision}`;
  // Bypass `request()` so a 409 returns a body instead of throwing.
  const base = typeof window === "undefined"
    ? (process.env.NEXT_PUBLIC_API_URL ?? "https://api-dev.usezombie.com")
    : "/backend";
  const res = await fetch(`${base}${url}`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Authorization: `Bearer ${token}`,
    },
    body,
  });
  const json = await res.json().catch(() => ({}));
  if (res.status === 200) {
    return { kind: "resolved", data: json as ResolveResponse };
  }
  if (res.status === 409) {
    return { kind: "already_resolved", data: json as AlreadyResolvedResponse };
  }
  throw new Error((json as { detail?: string }).detail ?? `Resolve failed: HTTP ${res.status}`);
}

export function approveApproval(workspaceId: string, gateId: string, token: string, reason?: string) {
  return resolveAction(workspaceId, gateId, "approve", token, reason);
}

export function denyApproval(workspaceId: string, gateId: string, token: string, reason?: string) {
  return resolveAction(workspaceId, gateId, "deny", token, reason);
}
