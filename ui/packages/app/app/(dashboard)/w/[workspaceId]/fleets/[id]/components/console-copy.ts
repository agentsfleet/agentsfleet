// The fleet console's operator-facing copy and enum values, single-sourced as
// named constants (RULE UFS). Every string the console pins in a test lives
// here so the test asserts against the constant, not a re-spelled literal, and
// a copy change lands in exactly one place.

// ── Column headings (§3 — the three questions the console answers) ──

export const COLUMN_IS_LABEL = "What it is";
export const COLUMN_DOES_LABEL = "What it does";
export const COLUMN_KNOWS_LABEL = "What it knows & costs";

// Sub-section labels within the columns.
export const TRIGGERS_SECTION_LABEL = "Triggers";
export const DANGER_ZONE_LABEL = "Danger zone";
export const APPROVALS_SECTION_LABEL = "Approvals";

// ── Left rail: the source editor (§4) ──

export const SOURCE_PANEL_TITLE = "Source";
export const SKILL_DOC_LABEL = "SKILL.md";
export const TRIGGER_DOC_LABEL = "TRIGGER.md";
export const TRIGGER_DOC_EMPTY = "No TRIGGER.md — this fleet has no declared triggers.";

export const EDIT_SOURCE_LABEL = "Edit";
export const CANCEL_EDIT_LABEL = "Cancel";
export const SAVE_SOURCE_LABEL = "Save changes";
export const SAVE_CONFIRM_LABEL = "Save";

// The save dialog's next-wake notice, pinned by its behavior test.
export const SAVE_NEXT_WAKE_NOTICE =
  "Takes effect on the next wake. In-flight runs finish on the current source. Memory is kept — same fleet_id.";
export const SAVE_DIALOG_TITLE = "Save source changes?";

// The "what changes when you save" preview.
export const DIFF_PANEL_TITLE = "What changes when you save";
export const DIFF_CURRENT_LABEL = "Current";
export const DIFF_PENDING_LABEL = "Pending";

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
export const MEMORY_FORGET_LABEL = "Forget";
export const MEMORY_FORGET_DIALOG_TITLE = "Forget this memory?";
export const MEMORY_FORGET_DIALOG_DESCRIPTION =
  "The fleet forgets this lesson on its next wake. This cannot be undone.";
export const MEMORY_FORGET_CONFIRM_LABEL = "Forget";
// Surfaced when the key was already gone (404) — the list is left unchanged.
export const MEMORY_FORGET_MISSING =
  "That memory was already gone — nothing to forget.";

// ── Right rail: the runs ledger and 7-day rollup (§6) ──

export const LEDGER_PANEL_TITLE = "Runs";
export const LEDGER_EMPTY_TITLE = "No runs yet";
export const LEDGER_EMPTY_DESCRIPTION = "Each wake the fleet records lands here.";
export const LEDGER_COST_UNKNOWN = "—";

export const ROLLUP_WINDOW_LABEL = "Latest 200 events in 7 days";
export const ROLLUP_WAKES_LABEL = "Wakes";
export const ROLLUP_TOKENS_LABEL = "Tokens";
export const ROLLUP_SPEND_LABEL = "Spend";
export const ROLLUP_FAILED_LABEL = "Failed";
export const ROLLUP_LIFETIME_LABEL = "Lifetime spend";
// Shown when the 7-day window fetch failed — the rollup degrades to the
// lifetime figure rather than blanking (Failure Modes: events page fetch fails).
export const ROLLUP_WINDOW_UNAVAILABLE =
  "Recent window unavailable — showing lifetime spend only.";

// The events window the client-side rollup covers (§6). One place so the label
// and the query string can never drift.
export const ROLLUP_WINDOW_SINCE = "7d";
export const ROLLUP_WINDOW_LIMIT = 200;

// ── Middle: the run-metrics strip (§3) ──

export const METRICS_STRIP_LABEL = "Latest run";
export const METRICS_TOKENS_LABEL = "Tokens";
export const METRICS_WALL_LABEL = "Wall";
export const METRICS_COST_LABEL = "Cost";
export const METRICS_COST_UNKNOWN = "—";
export const METRICS_EMPTY = "No runs recorded yet.";

// ── §7 — the delete confirm's memory trap (G8) ──

// The sentence the delete confirm gains so the operator learns delete destroys
// memory but editing keeps it. Pinned by test_delete_confirm_states_memory_trap.
export const DELETE_MEMORY_TRAP_NOTICE =
  "Its memory is deleted with it. Editing the source instead keeps everything it learned.";
