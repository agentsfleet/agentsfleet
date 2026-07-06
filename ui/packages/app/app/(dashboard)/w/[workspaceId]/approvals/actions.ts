"use server";

import { withToken, type ActionResult } from "@/lib/actions/with-token";
import {
  approveApproval as apiApproveApproval,
  denyApproval as apiDenyApproval,
  listApprovals as apiListApprovals,
  type ApprovalsListResponse,
  type ListApprovalsOpts,
  type ResolveOutcome,
} from "@/lib/api/approvals";

export async function listApprovalsAction(
  workspaceId: string,
  opts: ListApprovalsOpts = {},
): Promise<ActionResult<ApprovalsListResponse>> {
  return withToken((t) => apiListApprovals(workspaceId, t, opts));
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
