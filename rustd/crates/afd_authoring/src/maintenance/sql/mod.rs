//! Sweeps read `statement_timestamp()` and skip rows a live writer holds.

/// Closed drafts past retention with no live attempt and no installing fleet.
///
/// `$1` the closed status, `$2` the installing fleet status, `$3` limit.
pub(super) const CLOSED_DRAFTS: &str = include_str!("closed_drafts.sql");

/// Scrubs private dispatch results past their retention.
///
/// `$1` limit.
pub(super) const RESULTS: &str = include_str!("results.sql");

/// Live attempts whose lease or deadline passed; hints only, rechecked locked.
///
/// `$1` the active status, `$2` limit.
pub(super) const CANDIDATES: &str = include_str!("candidates.sql");

/// `$1` draft.
pub(super) const DRAFT: &str = include_str!("draft.sql");

/// `$1` attempt, `$2` the awaiting-accounting status.
pub(super) const EXPIRE: &str = include_str!("expire.sql");
