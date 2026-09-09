"use server";

import { withToken, type ActionResult } from "@/lib/actions/with-token";
import { APPROVAL_STATUS_ORDER, type ApprovalStatusTag } from "@/lib/api/approvals-types";
import {
  approveApproval as apiApproveApproval,
  denyApproval as apiDenyApproval,
  listApprovals as apiListApprovals,
  type ApprovalsListResponse,
  type ListApprovalsOpts,
  type ResolveOutcome,
} from "@/lib/api/approvals";

/**
 * Every status the table shows, in one call.
 *
 * The API narrows to ONE status per read, so the inbox needs five. Asking for
 * them as five server actions looked parallel — `Promise.all` at the call site —
 * and was not: Next runs Server Actions one at a time per client, queued to keep
 * their ordering meaningful, so the five became five sequential round trips,
 * each with its own token mint. Measured in the browser at 4.6s from load to
 * full table, every request starting within 5ms of the previous one finishing.
 *
 * Fanning out HERE is the same `Promise.all` against the same API, except the
 * concurrency is real and one token covers all five.
 *
 * All or nothing, like the reads it replaces: a single failed page would
 * silently shorten the table, so the first refusal is the whole result.
 */
export async function listAllApprovalsAction(
  workspaceId: string,
  opts: Omit<ListApprovalsOpts, "status"> = {},
  statuses: readonly ApprovalStatusTag[] = APPROVAL_STATUS_ORDER,
): Promise<ActionResult<ApprovalsListResponse>> {
  return withToken(async (token) => {
    const pages = await Promise.all(
      statuses.map((status) => apiListApprovals(workspaceId, token, { ...opts, status })),
    );
    return {
      items: pages.flatMap((page) => page.items),
      next_cursor: null,
    };
  });
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
