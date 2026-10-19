//! Operator-plane runner and history reads.
//!
//! The list and detail reads answer `statement_timestamp()` beside each row, so
//! liveness is derived against the database's clock: the same instant the
//! live-lease test inside the statement used, rather than a second clock read
//! in the daemon a few milliseconds later.

/// Page-stable runner total.
pub const COUNT_RUNNERS: &str = include_str!("count_runners.sql");

/// First runner page, newest first by the composite creation key. `$1` the
/// active lease status, `$2` the page size.
pub const LIST_RUNNERS_FIRST: &str = include_str!("list_runners_first.sql");

/// Runner page strictly after a composite cursor. `$1` the active lease
/// status, `$2` and `$3` the cursor, `$4` the page size.
pub const LIST_RUNNERS_AFTER: &str = include_str!("list_runners_after.sql");

/// One runner with current work counts and durable lifetime counters. `$1`
/// runner, `$2` the active lease status.
pub const RUNNER_DETAIL: &str = include_str!("runner_detail.sql");

/// Confirms the history owner exists.
pub const RUNNER_EXISTS: &str = include_str!("runner_exists.sql");

/// Page-stable history total. `$1` runner, `$2` the types asked for or NULL,
/// `$3` and `$4` the inclusive window, either end open.
pub const COUNT_EVENTS: &str = include_str!("count_events.sql");

/// First event page, newest first by the composite occurrence key. The
/// [`COUNT_EVENTS`] parameters, then `$5` the page size.
pub const LIST_EVENTS_FIRST: &str = include_str!("list_events_first.sql");

/// Event page strictly after a composite cursor. The [`COUNT_EVENTS`]
/// parameters, then `$5` and `$6` the cursor and `$7` the page size.
pub const LIST_EVENTS_AFTER: &str = include_str!("list_events_after.sql");
