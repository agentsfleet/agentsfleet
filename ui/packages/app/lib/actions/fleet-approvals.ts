"use server";

import type { ActionResult } from "@/lib/actions/with-token";
import type { ApprovalsListResponse, ListApprovalsOpts, ResolveOutcome } from "@/lib/api/approvals";
import { withToken } from "@/lib/actions/with-token";
import { approveApproval as apiApproveApproval, denyApproval as apiDenyApproval, listApprovals as apiListApprovals } from "@/lib/api/approvals";

/** Read fleet approval requests using the current authenticated session. */
export async function listApprovalsAction(
  workspaceId: string,
  opts: ListApprovalsOpts = {},
): Promise<ActionResult<ApprovalsListResponse>> {
  return withToken((token) => apiListApprovals(workspaceId, token, opts));
}

export async function approveApprovalAction(
  workspaceId: string,
  gateId: string,
  reason?: string,
): Promise<ActionResult<ResolveOutcome>> {
  return withToken((t) => apiApproveApproval(workspaceId, gateId, t, reason));
}

export async function denyApprovalAction(
  workspaceId: string,
  gateId: string,
  reason?: string,
): Promise<ActionResult<ResolveOutcome>> {
  return withToken((t) => apiDenyApproval(workspaceId, gateId, t, reason));
}
