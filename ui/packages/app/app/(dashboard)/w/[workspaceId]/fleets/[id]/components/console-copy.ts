// Coarse memory mutation outcome; private content never enters analytics.
export const OUTCOME = {
  success: "success",
  failure: "failure",
} as const;
export type Outcome = (typeof OUTCOME)[keyof typeof OUTCOME];

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
