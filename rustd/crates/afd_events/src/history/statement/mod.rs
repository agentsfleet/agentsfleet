//! The texts the read side runs, one `.sql` file each.
//!
//! Split out of [`super`] because the statements are DATA — long, exact, and
//! read as a unit — while the methods beside them are control flow. Each file
//! is schema-qualified (RULE NSQ) and checked against the rebuilt schema by
//! `afd_db`'s `test_every_sql_file_prepares`.
//!
//! # One column list, written into every file and pinned by a test
//!
//! Every read selects the same fourteen columns in the same order, and the
//! detail and thread reads append the three bodies after them. A whole file
//! cannot splice a shared fragment, so the list is written into each and
//! [`tests`] asserts every text carries it verbatim: a column added to one
//! file and not the others fails there, not on a deployment.
//!
//! `cost_nanos` is a correlated subselect rather than a `LEFT JOIN`: billing
//! writes up to two charge rows per event — unique on
//! `(event_id, charge_type, fleet_id)` — so a join would duplicate the event
//! row per leg and a page of 50 would render as 100.
//!
//! # One text per scope, cursor and actor filter
//!
//! A bound behind `IS NULL OR` cannot be decided by a generic plan, so nothing
//! here is gated: the scope and cursor stay index conditions, and an absent
//! actor filter is an absent predicate. Each text numbers its placeholders in
//! the order `History` binds them: workspace, fleet when fleet-scoped, the
//! cursor pair when resuming, the actor pattern when filtering, the `since`
//! pair, and the limit.
//!
//! # `since` is resolved by the database
//!
//! Every listing bounds `created_at >= COALESCE($at, statement_timestamp() -
//! $ago, '-infinity')`: an absolute instant, a span measured back from the
//! statement's own clock, or neither. The expression is stable within the
//! statement, so it stays an index condition under a generic plan.

/// A fleet's first page: `$3` since instant, `$4` since span, `$5` limit.
pub(super) const SELECT_FLEET_PAGE: &str = include_str!("select_fleet_page.sql");

/// A fleet's first page by actor: `$3` actor LIKE, `$4` since instant, `$5`
/// since span, `$6` limit.
pub(super) const SELECT_FLEET_PAGE_BY_ACTOR: &str = include_str!("select_fleet_page_by_actor.sql");

/// A fleet's page after a cursor: `$3` cursor instant, `$4` cursor event id,
/// `$5` since instant, `$6` since span, `$7` limit.
pub(super) const SELECT_FLEET_PAGE_AFTER: &str = include_str!("select_fleet_page_after.sql");

/// A fleet's page by actor after a cursor: `$3` cursor instant, `$4` cursor
/// event id, `$5` actor LIKE, `$6` since instant, `$7` since span, `$8` limit.
pub(super) const SELECT_FLEET_PAGE_AFTER_BY_ACTOR: &str =
    include_str!("select_fleet_page_after_by_actor.sql");

/// A workspace's first page: `$2` since instant, `$3` since span, `$4` limit.
pub(super) const SELECT_WORKSPACE_PAGE: &str = include_str!("select_workspace_page.sql");

/// A workspace's first page by actor: `$2` actor LIKE, `$3` since instant,
/// `$4` since span, `$5` limit.
pub(super) const SELECT_WORKSPACE_PAGE_BY_ACTOR: &str =
    include_str!("select_workspace_page_by_actor.sql");

/// A workspace's page after a cursor: `$2` cursor instant, `$3` cursor event
/// id, `$4` since instant, `$5` since span, `$6` limit.
pub(super) const SELECT_WORKSPACE_PAGE_AFTER: &str =
    include_str!("select_workspace_page_after.sql");

/// A workspace's page by actor after a cursor: `$2` cursor instant, `$3`
/// cursor event id, `$4` actor LIKE, `$5` since instant, `$6` since span, `$7`
/// limit.
pub(super) const SELECT_WORKSPACE_PAGE_AFTER_BY_ACTOR: &str =
    include_str!("select_workspace_page_after_by_actor.sql");

/// The listing text for a read's scope, cursor and actor filter — the same
/// three facts `History` decides its binds from, so the two cannot disagree
/// about which placeholders a text numbers.
pub(super) const fn listing_text(
    fleet_scoped: bool,
    resumes: bool,
    by_actor: bool,
) -> &'static str {
    match (fleet_scoped, resumes, by_actor) {
        (true, false, false) => SELECT_FLEET_PAGE,
        (true, false, true) => SELECT_FLEET_PAGE_BY_ACTOR,
        (true, true, false) => SELECT_FLEET_PAGE_AFTER,
        (true, true, true) => SELECT_FLEET_PAGE_AFTER_BY_ACTOR,
        (false, false, false) => SELECT_WORKSPACE_PAGE,
        (false, false, true) => SELECT_WORKSPACE_PAGE_BY_ACTOR,
        (false, true, false) => SELECT_WORKSPACE_PAGE_AFTER,
        (false, true, true) => SELECT_WORKSPACE_PAGE_AFTER_BY_ACTOR,
    }
}

/// One event by its identifier, scoped to the workspace and fleet that own it.
///
/// The scoping is in the STATEMENT rather than checked after the read: a row
/// belonging to another workspace must not come back and then be filtered, or
/// the filter becomes the only thing standing between two tenants. Bounded by
/// construction — the predicate names the table's unique pair beside the
/// workspace.
///
/// `$1` workspace, `$2` fleet, `$3` event.
pub(super) const SELECT_DETAIL: &str = include_str!("select_detail.sql");

/// A thread's first page, bodies included: `$1` workspace, `$2` fleet, `$3`
/// limit.
pub(super) const SELECT_THREAD_PAGE: &str = include_str!("select_thread_page.sql");

/// A thread's page after a cursor: `$3` cursor instant, `$4` cursor event id,
/// `$5` limit.
pub(super) const SELECT_THREAD_PAGE_AFTER: &str = include_str!("select_thread_page_after.sql");

/// A thread's finished rows before a cursor, the turns a chat lease carries:
/// `$3` cursor instant, `$4` cursor event id, `$5` and `$6` the two finished
/// statuses, `$7` limit. The statuses are a predicate so the limit counts
/// finished rows: running or refused rows among the newest would otherwise
/// shrink the window while older finished turns exist.
pub(super) const SELECT_THREAD_FINISHED_BEFORE: &str =
    include_str!("select_thread_finished_before.sql");

// One actor's texts and the waiting read's live beside this file.
mod actor;
pub(super) use self::actor::{SELECT_FLEET_PAGE_OF_ACTOR, SELECT_FLEET_PAGE_OF_ACTOR_AFTER};

mod queued;
#[cfg(feature = "test-util")]
pub use self::queued::QUEUED_READ_TEXTS;
pub(super) use self::queued::{SELECT_THREAD_QUEUED, SELECT_THREAD_QUEUED_AFTER};

/// Every listing and thread text, named, for the suite that asks Postgres how
/// it plans each one.
#[cfg(feature = "test-util")]
pub const READ_TEXTS: [(&str, &str); 13] = [
    ("fleet page", SELECT_FLEET_PAGE),
    ("fleet page by actor", SELECT_FLEET_PAGE_BY_ACTOR),
    ("fleet page after", SELECT_FLEET_PAGE_AFTER),
    (
        "fleet page after by actor",
        SELECT_FLEET_PAGE_AFTER_BY_ACTOR,
    ),
    ("workspace page", SELECT_WORKSPACE_PAGE),
    ("workspace page by actor", SELECT_WORKSPACE_PAGE_BY_ACTOR),
    ("workspace page after", SELECT_WORKSPACE_PAGE_AFTER),
    (
        "workspace page after by actor",
        SELECT_WORKSPACE_PAGE_AFTER_BY_ACTOR,
    ),
    ("thread page", SELECT_THREAD_PAGE),
    ("thread page after", SELECT_THREAD_PAGE_AFTER),
    ("thread finished before", SELECT_THREAD_FINISHED_BEFORE),
    ("fleet page of actor", SELECT_FLEET_PAGE_OF_ACTOR),
    (
        "fleet page of actor after",
        SELECT_FLEET_PAGE_OF_ACTOR_AFTER,
    ),
];

#[cfg(test)]
mod tests;
