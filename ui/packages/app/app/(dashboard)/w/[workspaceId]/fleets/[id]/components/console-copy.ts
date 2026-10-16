// The fleet console's operator-facing copy and enum values, single-sourced as
// named constants (RULE UFS). Every string the console pins in a test lives
// here so the test asserts against the constant, not a re-spelled literal, and
// a copy change lands in exactly one place.

// ── Header: the way back to the wall ──

// The console's escape hatch is the breadcrumb's first crumb. It carries a
// landmark so a screen reader can reach it directly and so the crumb is
// distinguishable from the identically-named sidebar destination.
export const BREADCRUMB_LABEL = "Breadcrumb";
export const FLEETS_CRUMB_LABEL = "Fleets";

// Coarse memory mutation outcome; private content never enters analytics.
export const OUTCOME = {
  success: "success",
  failure: "failure",
} as const;
export type Outcome = (typeof OUTCOME)[keyof typeof OUTCOME];

// ── Right rail: the memory panel (§5) ──

export const MEMORY_PANEL_TITLE = "Memory";
export const MEMORY_EMPTY_TITLE = "Nothing learned yet";
export const MEMORY_EMPTY_DESCRIPTION =
  "Durable lessons the fleet records appear here. Forget one to correct it.";
export const MEMORY_FETCH_UNAVAILABLE = "Memory is temporarily unavailable. Try refreshing the page.";
export const MEMORY_FORGET_LABEL = "Forget";
export const MEMORY_FORGET_DIALOG_TITLE = "Forget this memory?";
export const MEMORY_FORGET_DIALOG_DESCRIPTION =
  "The fleet forgets this lesson on its next wake. This cannot be undone.";
export const MEMORY_FORGET_CONFIRM_LABEL = "Forget";
// Surfaced when the key was already gone (404) — the list is left unchanged.
export const MEMORY_FORGET_MISSING =
  "That memory was already gone — nothing to forget.";

// ── Right rail: the runs ledger and 7-day rollup (§6) ──

// ── Chat summary ──

export const METRICS_STRIP_LABEL = "Fleet summary";
export const METRICS_STATUS_LABEL = "Status";
export const METRICS_OUTCOME_LABEL = "Latest outcome";
export const METRICS_TOKENS_LABEL = "Tokens";
export const METRICS_TIME_LABEL = "Duration";
// Rides beside the duration the way `tok` rides beside the token count.
// Every figure on the strip belongs to the LATEST run -- not a lifetime
// total -- and the duration is the one a reader was most likely to take
// for an uptime or an age.
export const METRICS_TIME_UNIT = "run";
export const METRICS_COST_LABEL = "Spend";
// Any missing figure (tokens, time, or cost) renders a dash — an unknown is
// never a fabricated zero.
export const METRICS_VALUE_UNKNOWN = "—";
export const METRICS_EMPTY = "No outcome recorded yet.";
export const METRICS_UNAVAILABLE = "Latest data unavailable.";
export const METRICS_APPROVAL_LABEL = "approval waiting";
export const METRICS_APPROVALS_LABEL = "approvals waiting";

// ── §7 — the delete confirm's memory trap (G8) ──

// The sentence the delete confirm gains so the operator learns delete destroys
// memory but editing keeps it. Pinned by test_delete_confirm_states_memory_trap.
export const DELETE_MEMORY_TRAP_NOTICE =
  "Its memory goes with it — improving the setup keeps what it learned. Are you sure?";
