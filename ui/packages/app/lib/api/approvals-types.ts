// The approval decision tags and page size the inbox and its buttons share. Dependency-free on purpose: client components read these
// without pulling the transport, whose retry policy is server-only.

/** How many gates one inbox page carries — the first render, each poll, and
 * every "load more" ask for the same page. */
export const APPROVALS_PAGE_LIMIT = 50;

// Wire-protocol values the API understands for the `/approve` / `/deny` POST
// paths. Single source of truth — dashboard components import these so the
// decision flow has one place that pins the literal.
export const APPROVAL_DECISION = {
  APPROVE: "approve",
  DENY: "deny",
} as const;

export type ApprovalDecision = typeof APPROVAL_DECISION[keyof typeof APPROVAL_DECISION];
