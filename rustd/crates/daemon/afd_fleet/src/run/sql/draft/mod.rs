//! The draft kind's own run statement. Its claim, renewal and settlement run
//! the shared statements ([`super::shared`]); the draft's rows are
//! `afd_setup::setup_run`'s.

/// A draft-owned run's pricing, attribution and fence facts.
///
/// `$1` run, `$2` runner. No row for a fleet's run or another runner's.
pub const SELECT_DRAFT_RUN: &str = include_str!("select_draft_run.sql");
