import {
  DEFAULT_REQUEST_TIMEOUT_MS,
  classifyTransportFailure,
  isTransportInterrupt,
  request,
  requireApiOrigin,
} from "./client";

import { ApiError } from "./errors";
import { APPROVAL_DECISION } from "./approvals-types";
import type { ApprovalDecision } from "./approvals-types";
import { confirmedApprovalResolution } from "./approval-resolution";

// Mirrors the server's inbox gate row
// (`GateRow` in rustd/crates/daemon/afd_approval/src/inbox/row.rs)
// verbatim — no shim, no rename. Renders the same shape the dashboard queries.

export type ApprovalStatus = "pending" | "approved" | "denied" | "answered" | "timed_out" | "archived";

export type ApprovalStatusValue = ApprovalStatus | (string & {});

export type ApprovalGate = {
  gate_id: string;
  fleet_id: string;
  fleet_name: string;
  workspace_id: string;
  action_id: string;
  tool_name: string;
  action_name: string;
  gate_type: string;
  proposed_action: string;
  evidence: Record<string, unknown>;
  blast_radius: string;
  status: ApprovalStatusValue;
  detail: string;
  /** RFC 3339 instant */
  created_at: string;
  /**
   * RFC 3339 instant — the answer's deadline, for display only. Nothing
   * closes the gate when it passes: the read reports `timed_out`, and the gate
   * still takes an approval, a denial or an answer.
   */
  expires_at: string;
  /** RFC 3339 instant; null when still pending */
  resolved_at: string | null;
  resolved_by: string;
  /**
   * The decider's name, joined from this deployment's own user rows. Empty
   * while pending, for the daemon's sentinels, and for a subject with no row —
   * all three render as the shortened subject.
   */
  resolved_by_name: string;
};

export type ApprovalsListResponse = {
  items: ApprovalGate[];
  next_cursor: string | null;
};

export type ResolveResponse = {
  gate_id: string;
  action_id: string;
  outcome: ApprovalStatus;
  resolved_at: string;
  resolved_by: string;
};

export type AlreadyResolvedResponse = ResolveResponse & {
  error_code: "UZ-APPROVAL-006";
  detail: string;
};

export type ResolveOutcome =
  | { kind: "resolved"; data: ResolveResponse }

  | { kind: "already_resolved"; data: AlreadyResolvedResponse };

// The decision's body: an optional note on approve and deny, the answer itself
// on a question gate.
type ResolveFields = {reason?: string; answer?: string};

export type ListApprovalsOpts = {
  status?: string;
  fleetId?: string;
  gateType?: string;
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
  if (opts.gateType) params.set("gate_type", opts.gateType);
  if (opts.cursor) params.set("cursor", opts.cursor);
  if (opts.limit != null) params.set("limit", String(opts.limit));
  const qs = params.toString();
  const path = qs
    ? `/v1/workspaces/${workspaceId}/approvals?${qs}`
    : `/v1/workspaces/${workspaceId}/approvals`;
  return request<ApprovalsListResponse>(path, { method: "GET" }, token);
}



// Resolve. The server returns 200 with ResolveResponse on success and 409 with
// AlreadyResolvedResponse when another channel got there first. Both are
// expected outcomes from the operator's perspective — we surface them to the
// caller as a tagged union instead of throwing on 409.
async function resolveAction(
  workspaceId: string,
  gateId: string,
  decision: ApprovalDecision,
  token: string,
  fields: ResolveFields,
): Promise<ResolveOutcome> {
  const body = JSON.stringify(fields);
  // `/decision` as its own segment, not `:decision`. The daemon's router binds
  // one parameter per path segment and refuses a literal after it, so the
  // custom verb cannot ride the gate identifier — see `WorkspaceRoute::
  // ApprovalResolve` in `rustd/crates/daemon/afd_api`, which is the served spelling.
  const url = `/v1/workspaces/${workspaceId}/approvals/${gateId}/${decision}`;
  // Bypass `request()` so a 409 returns a body instead of throwing — but not
  // the transport's timeout. Without one a hung resolve never settles: the
  // inbox's optimistic row stays hidden with no error, and every later Server
  // Action from that tab waits behind it in the router's action queue. A
  // timeout here classifies exactly as it does in client.ts, so `withToken`
  // hands the row back with the same `TIMEOUT` code.
  const base = typeof window === "undefined" ? requireApiOrigin() : "/backend";
  let res: Response;
  try {
    res = await fetch(`${base}${url}`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        Authorization: `Bearer ${token}`,
      },
      body,
      signal: AbortSignal.timeout(DEFAULT_REQUEST_TIMEOUT_MS),
    });
  } catch (cause) {
    throw classifyTransportFailure(cause, url);
  }
  const json = await res.json().catch((cause: unknown) => {
    if (isTransportInterrupt(cause)) throw classifyTransportFailure(cause, url);
    return {};
  });
  if (res.status === 200 || res.status === 409) {
    const outcome = confirmedApprovalResolution({kind: res.status === 200 ? "resolved" : "already_resolved", data: json}, gateId, decision);
    if (outcome) return outcome;
    // A 409 that is no competing decision is a refusal, such as an archived
    // fleet's `UZ-AGT-020`; it falls through to the problem body below.
    if (res.status === 200) throw new ApiError("The decision could not be confirmed. Review the request's current status before trying again.", 502, "UZ-UNKNOWN");
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
  return resolveAction(workspaceId, gateId, APPROVAL_DECISION.APPROVE, token, reason ? {reason} : {});
}

export function denyApproval(workspaceId: string, gateId: string, token: string, reason?: string) {
  return resolveAction(workspaceId, gateId, APPROVAL_DECISION.DENY, token, reason ? {reason} : {});
}

/** Answer a `question` gate; the daemon refuses `approve` on one. */
export function answerApproval(workspaceId: string, gateId: string, token: string, answer: string) {
  return resolveAction(workspaceId, gateId, APPROVAL_DECISION.ANSWER, token, {answer});
}
