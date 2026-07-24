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

// ── Left rail: the source editor (§4) ──

export const SOURCE_PANEL_TITLE = "Source";
export const SKILL_SOURCE_PANEL_TITLE = "Skill source";
export const TRIGGER_SOURCE_PANEL_TITLE = "Trigger source";
export const SKILL_DOC_LABEL = "SKILL.md";
export const TRIGGER_DOC_LABEL = "TRIGGER.md";
export const TRIGGER_DOC_EMPTY = "No TRIGGER.md — this fleet has no declared triggers.";

export const EDIT_SOURCE_LABEL = "Edit";
export const CANCEL_EDIT_LABEL = "Cancel";
// The source card is collapsed by default — the steer thread is the point of
// the page, so the heavy document viewer appears only on request. Editing pins
// the card open (no collapse control while a draft exists).
export const VIEW_SOURCE_LABEL = "View source";
export const HIDE_SOURCE_LABEL = "Hide source";
export const SAVE_SOURCE_LABEL = "Save changes";
export const SAVE_CONFIRM_LABEL = "Save";

// The save dialog's next-wake notice, pinned by its behavior test.
export const SAVE_NEXT_WAKE_NOTICE =
  "Changes apply the next time this fleet handles something. Work already running keeps using the previous version. Its memory stays available.";
export const SAVE_DIALOG_TITLE = "Save source changes?";

// Shown after a 412: another operator saved while this editor was open, so the
// current source was reloaded while the operator's pending edit was kept.
export const SAVE_STALE_RELOADED_NOTICE =
  "This source changed while you were editing. Compare your pending edit with the latest version before saving again.";

// Which document a save touched — the `field` value on fleet_source_saved.
export const SOURCE_FIELD = {
  skill: "skill",
  trigger: "trigger",
} as const;
export type SourceField = (typeof SOURCE_FIELD)[keyof typeof SOURCE_FIELD];

// Coarse analytics outcome shared by fleet_source_saved and
// fleet_memory_forgotten (no content, no key — just success/failure).
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
export const METRICS_COST_LABEL = "Spend";
// Any missing figure (tokens, time, or cost) renders a dash — an unknown is
// never a fabricated zero.
export const METRICS_VALUE_UNKNOWN = "—";
export const METRICS_EMPTY = "No outcome recorded yet.";
export const METRICS_UNAVAILABLE = "Latest data unavailable.";
export const METRICS_APPROVALS_UNAVAILABLE = "Approvals unavailable";
export const METRICS_APPROVAL_LABEL = "approval waiting";
export const METRICS_APPROVALS_LABEL = "approvals waiting";

// ── §7 — the delete confirm's memory trap (G8) ──

// The sentence the delete confirm gains so the operator learns delete destroys
// memory but editing keeps it. Pinned by test_delete_confirm_states_memory_trap.
export const DELETE_MEMORY_TRAP_NOTICE =
  "Its memory is deleted with it. Editing the source instead keeps everything it learned.";
