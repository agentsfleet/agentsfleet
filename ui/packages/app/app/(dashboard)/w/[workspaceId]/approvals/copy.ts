// The Approvals surface's user-facing strings, in one place so the filter hint,
// the column headers and the chips cannot drift from the grammar the parser
// actually implements.

export const APPROVALS_PAGE_DESCRIPTION = "Fleet actions that pause for human review.";
export const APPROVALS_SECTION_LABEL = "Review approvals";
export const APPROVALS_TABLE_CAPTION = "Approvals and their outcomes";


// The agent callsign carries the same prefix the Fleets tile prints, so one
// fleet reads identically on both surfaces (RULE UFS).
export const AGENT_PREFIX = "Agent";

export const REQUEST_COLUMN_HEADER = "Request";
export const FLEET_COLUMN_HEADER = "Fleet";
export const REQUESTED_COLUMN_HEADER = "Requested";
export const AUTO_DENY_COLUMN_HEADER = "Auto-deny";
export const ACTIONS_COLUMN_HEADER = "Actions";
export const STATUS_COLUMN_HEADER = "Status";
export const DECIDED_COLUMN_HEADER = "Decided";

export const APPROVE_LABEL = "Approve";
export const DENY_LABEL = "Deny";
export const REFRESH_LABEL = "Refresh";
export const REFRESHING_LABEL = "Refreshing…";
export const REFRESHED_LABEL = "Refreshed";

// The five states a row can be in, as the status cell spells them. Two are
// nobody's verdict: `timed_out` is the deadline passing unanswered, and
// `auto_killed` is the daemon stopping the fleet and closing its open questions
// with it — so the cell names them rather than implying a person decided.
export const STATUS_LABEL = {
  pending: "Pending",
  approved: "Approved",
  denied: "Denied",
  timed_out: "Timed out",
  auto_killed: "Auto-killed",
} as const;

// The colour each state carries. Pending is the only one asking for anything,
// so it is the only warm one; the two nobody chose are muted rather than red,
// because a deadline passing is not a refusal.
export const STATUS_VARIANT = {
  pending: "warn",
  approved: "green",
  denied: "destructive",
  timed_out: "orange",
  auto_killed: "default",
} as const;

/** What a pending row shows where a resolved one shows who decided it. */
export const AWAITING_DECISION = "Awaiting review";





// Denying is irreversible product-wide: the grant row is UPDATEd to revoked and
// kept, `ENSURE_GRANT` will not reset it, and `REQUEST_GRANT` only raises a card
// for a PENDING grant. Reconnecting the connector does not help — that is the
// vault, not this table. The only way back is a fresh install under a new fleet
// id. So the row asks first, the way Secrets asks before a delete.
export const DENY_CONFIRM_TITLE = "Deny this grant?";
export const DENY_CONFIRM_BODY =
  "This cannot be undone. The fleet's parked event ends, and no new card can be raised for this grant — the fleet has to be installed again to ask a second time.";

export const NO_APPROVALS_TITLE = "No approvals yet";
export const NO_APPROVALS_DESCRIPTION =
  "Requests your fleets raise for human review appear here, and stay as the record of what was decided.";
