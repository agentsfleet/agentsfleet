// Approval-gate wire literals.
//
// The daemon models a decision as its own path segment
// (`/approvals/{gate_id}/{decision}`) carrying its own capability, so the two
// decisions are separate strings on the wire and separate subcommands here.
// Declared once (RULE UFS): the command builds the path from these, the tests
// assert against these, and a rename cannot land on one side only.

export const GATE_DECISION = {
  approve: "approve",
  deny: "deny",
} as const;

export type GateDecision = (typeof GATE_DECISION)[keyof typeof GATE_DECISION];

// Gate lifecycle as the daemon reports it. `pending` is the only state a
// decision may be posted against; the other two are terminal.
export const GATE_STATUS = {
  pending: "pending",
  approved: "approved",
  denied: "denied",
} as const;

// Column labels for the gate table. Shared with the tests so a header rename
// cannot pass a test that hard-codes the old word.
export const GATE_COLUMN = {
  gate: "GATE",
  fleet: "FLEET",
  kind: "KIND",
  status: "STATUS",
  action: "ACTION",
} as const;

// The gate's lifecycle field. One spelling, because the daemon uses it twice:
// as the row's own key and as the query parameter that narrows a listing.
const FIELD_STATUS = "status" as const;

export const GATE_FIELD = {
  gate: "gate",
  fleet: "fleet",
  kind: "kind",
  status: FIELD_STATUS,
  action: "action",
} as const;

// One em dash stands in for an absent optional field, matching every other
// table this CLI prints.

// Query parameters the approvals route serves. The Fleet and status filters
// are the daemon's own, so narrowing happens there rather than over a page this
// client already truncated — a client-side filter over one page of 50 reports
// "nothing waiting" for a workspace whose gate is on page two.
export const GATE_QUERY = {
  limit: "limit",
  cursor: "cursor",
  status: FIELD_STATUS,
  fleetId: "fleet_id",
} as const;

// The daemon caps `limit` at 200 and defaults to 50; asking for the cap is
// fewer round trips for the same answer.
export const GATE_PAGE_LIMIT = 200;
export const GATE_MAX_PAGES = 50;
