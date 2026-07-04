import { request } from "./client";
import { ApiError } from "./errors";

// Mirrors the server's PendingRow envelope
// (src/agentsfleetd/fleet_runtime/approval_gate_db_reads.zig)
// verbatim — no shim, no rename. Renders the same shape the dashboard queries.

export type ApprovalStatus = "pending" | "approved" | "denied" | "timed_out" | "auto_killed";
export type ApprovalStatusValue = ApprovalStatus | (string & {});

export type ApprovalGate = {
  gate_id: string;
  fleet_id: string;
  fleet_name: string;
  workspace_id: string;
  action_id: string;
  tool_name: string;
  action_name: string;
  gate_kind: string;
  proposed_action: string;
  evidence: Record<string, unknown>;
  blast_radius: string;
  status: ApprovalStatusValue;
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
  fleetId?: string;
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
  if (opts.fleetId) params.set("fleet_id", opts.fleetId);
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

// Wire-protocol values the API understands for the `:approve` / `:deny` POST
// paths. Single source of truth — dashboard components import these so the
// decision flow has one place that pins the literal.
export const APPROVAL_DECISION = {
  APPROVE: "approve",
  DENY: "deny",
} as const;
export type ApprovalDecision = typeof APPROVAL_DECISION[keyof typeof APPROVAL_DECISION];

// Resolve. The server returns 200 with ResolveResponse on success and 409 with
// AlreadyResolvedResponse when another channel got there first. Both are
// expected outcomes from the operator's perspective — we surface them to the
// caller as a tagged union instead of throwing on 409.
async function resolveAction(
  workspaceId: string,
  gateId: string,
  decision: ApprovalDecision,
  token: string,
  reason?: string,
): Promise<ResolveOutcome> {
  const body = JSON.stringify(reason ? { reason } : {});
  const url = `/v1/workspaces/${workspaceId}/approvals/${gateId}:${decision}`;
  // Bypass `request()` so a 409 returns a body instead of throwing.
  const base = typeof window === "undefined"
    ? (process.env.NEXT_PUBLIC_API_URL ?? "https://api-dev.agentsfleet.net")
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
  // RFC 7807 problem+json, same shape `request()` throws on — see client.ts.
  // A bare `Error` here would discard `error_code` before `presentErrorString`
  // ever runs (withToken only reads `.code` off an `ApiError` instance).
  // `user_message` (curated dashboard-safe copy) is preferred over `detail`/
  // `title`, same precedence as client.ts's request().
  const errBody = json as {
    detail?: string;
    title?: string;
    error_code?: string;
    request_id?: string;
    user_message?: string;
  };
  throw new ApiError(
    errBody.user_message ?? errBody.detail ?? errBody.title ?? `Resolve failed: HTTP ${res.status}`,
    res.status,
    errBody.error_code ?? "UZ-UNKNOWN",
    errBody.request_id,
  );
}

export function approveApproval(workspaceId: string, gateId: string, token: string, reason?: string) {
  return resolveAction(workspaceId, gateId, APPROVAL_DECISION.APPROVE, token, reason);
}

export function denyApproval(workspaceId: string, gateId: string, token: string, reason?: string) {
  return resolveAction(workspaceId, gateId, APPROVAL_DECISION.DENY, token, reason);
}
