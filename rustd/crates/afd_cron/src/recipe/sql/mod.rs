//! Every inheritance write runs on the fleet setup transaction.
//!
//! The parent lock and the independent count are shared with the store, so
//! they live in [`crate::sql`]; these are the statements only a recipe
//! replacement runs.

/// The fleet's recipe-owned schedules, locked, each with whether a live syncer
/// holds it — judged on `clock_timestamp()`, after the lock wait.
///
/// The clock is read outside the locking `MATERIALIZED` step. Inside it, the
/// select list is computed before the lock is granted, and a holder that only
/// locked the row triggers no re-check, so a lease that lapsed during the wait
/// would still read as held.
pub(super) const ROWS: &str = include_str!("rows.sql");

/// A recipe slot's first schedule, unclaimed and waiting for its first sync.
pub(super) const INSERT: &str = include_str!("insert.sql");

/// A recipe slot's schedule moved to its new base, releasing any fence.
pub(super) const UPDATE: &str = include_str!("update.sql");

/// A removed slot's schedule, marked for deletion once the scheduler agrees.
pub(super) const RETIRE: &str = include_str!("retire.sql");
