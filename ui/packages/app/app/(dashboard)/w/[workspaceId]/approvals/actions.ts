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

/**
 * The inbox, re-read on demand: one token, one request, one query.
 *
 * It took five of each until M194. The API narrowed to ONE status per read and
 * an absent `?status=` meant `pending` rather than "no filter", so a table
 * showing every state had to ask once per state. Asking as five Server Actions
 * looked parallel — `Promise.all` at the call site — and was not: Next runs
 * Server Actions one at a time per client, queued to keep their ordering
 * meaningful. Measured in the browser at 4.6s from load to full table, every
 * request starting within 5ms of the previous one finishing.
 *
 * Fanning out on the server fixed the round trips and left five SQL queries
 * behind one call. The fix is upstream of both: `?status=` is a filter now, so
 * omitting it returns every state from a single statement.
 *
 * The load does not come through here at all — the page server-renders it. This
 * is the Refresh button's path, and the re-read after a resolve.
 */
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
