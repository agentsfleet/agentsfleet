// Wire-protocol values the API understands for the `/approve`, `/deny` and
// `/answer` POST paths. Single source of truth — dashboard components import
// these so the decision flow has one place that pins the literal.
export const APPROVAL_DECISION = {
  APPROVE: "approve",
  DENY: "deny",
  ANSWER: "answer",
} as const;

export type ApprovalDecision = typeof APPROVAL_DECISION[keyof typeof APPROVAL_DECISION];

// The states a gate row can be in, as the API spells them
// (`afd_api_wire::approval::status`). Declared here beside the decision tags so the
// inbox's client components can name a status without pulling the transport.
//
// Three are decisions a person makes. `timed_out` is the deadline passing with
// no answer; nothing closes the gate then, so it can still be approved, denied
// or answered. `archived` is the fleet being archived or deleted while the
// gate waited, which closes its open questions with it.
export const APPROVAL_STATUS = {
  PENDING: "pending",
  APPROVED: "approved",
  DENIED: "denied",
  ANSWERED: "answered",
  TIMED_OUT: "timed_out",
  ARCHIVED: "archived",
} as const;

// A `question` gate is a fleet run asking the person something; its
// `proposed_action` holds the question, and it takes an answer, not approval.
export const APPROVAL_GATE_TYPE = {
  QUESTION: "question",
} as const;
