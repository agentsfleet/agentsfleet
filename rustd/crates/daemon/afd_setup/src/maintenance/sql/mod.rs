//! Sweeps read `statement_timestamp()` and skip rows a live writer holds.

/// Closed drafts past retention with no live run and no installing fleet.
///
/// `$1` the closed status, `$2` the installing fleet status, `$3` limit.
pub(super) const CLOSED_DRAFTS: &str = include_str!("closed_drafts.sql");
