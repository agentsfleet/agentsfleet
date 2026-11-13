// Wire-protocol values the API understands for the `/approve` / `/deny` POST
// paths. Single source of truth — dashboard components import these so the
// decision flow has one place that pins the literal.
export const APPROVAL_DECISION = {
  APPROVE: "approve",
  DENY: "deny",
} as const;

export type ApprovalDecision = typeof APPROVAL_DECISION[keyof typeof APPROVAL_DECISION];

// The five states a gate row can be in, as the API spells them
// (`afd_api_wire::approval::status`). Declared here beside the decision tags so the
// inbox's client components can name a status without pulling the transport.
//
// Only two are decisions a person makes. `timed_out` is the deadline passing
// with no answer; `archived` is the fleet being archived or deleted while the
// gate waited, which closes its open questions with it. Neither is anybody's
// verdict, which is why the resolve vocabulary has three arms and this one
// has five.
export const APPROVAL_STATUS = {
  PENDING: "pending",
  APPROVED: "approved",
  DENIED: "denied",
  TIMED_OUT: "timed_out",
  ARCHIVED: "archived",
} as const;
