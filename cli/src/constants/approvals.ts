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

export type GateStatus = (typeof GATE_STATUS)[keyof typeof GATE_STATUS];

// Column labels for the gate table. Shared with the tests so a header rename
// cannot pass a test that hard-codes the old word.
export const GATE_COLUMN = {
  gate: "GATE",
  fleet: "FLEET",
  kind: "KIND",
  status: "STATUS",
  action: "ACTION",
} as const;

export const GATE_FIELD = {
  gate: "gate",
  fleet: "fleet",
  kind: "kind",
  status: "status",
  action: "action",
} as const;

// One em dash stands in for an absent optional field, matching every other
// table this CLI prints.
export const EMPTY_CELL = "—" as const;
