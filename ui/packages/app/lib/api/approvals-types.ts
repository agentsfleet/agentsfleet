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

// The five states a gate row can be in, as the API spells them
// (`afd_wire::approval::status`). Declared here beside the decision tags so the
// inbox's client components can name a status without pulling the transport.
//
// Only two are decisions a person makes. `timed_out` is the deadline passing
// with no answer; `auto_killed` is the daemon stopping the fleet and closing its
// open questions with it — neither is anybody's verdict, which is why the
// resolve vocabulary has three arms and this one has five.
export const APPROVAL_STATUS = {
  PENDING: "pending",
  APPROVED: "approved",
  DENIED: "denied",
  TIMED_OUT: "timed_out",
  AUTO_KILLED: "auto_killed",
} as const;

export type ApprovalStatusTag = typeof APPROVAL_STATUS[keyof typeof APPROVAL_STATUS];

/** Tab order: the queue first, then the answers, then the two nobody gave. */
export const APPROVAL_STATUS_ORDER = [
  APPROVAL_STATUS.PENDING,
  APPROVAL_STATUS.APPROVED,
  APPROVAL_STATUS.DENIED,
  APPROVAL_STATUS.TIMED_OUT,
  APPROVAL_STATUS.AUTO_KILLED,
] as const;

/** A row at this status is finished: no approve or deny applies to it. */
export function isResolved(status: ApprovalStatusTag): boolean {
  return status !== APPROVAL_STATUS.PENDING;
}
