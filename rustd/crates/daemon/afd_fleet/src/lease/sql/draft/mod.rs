//! The draft kind's own lease statement. Its claim, renewal and settlement run
//! the shared statements ([`super::shared`]); the draft's rows are
//! `afd_setup::setup_lease`'s.

/// A draft-owned lease's pricing, attribution and fence facts.
///
/// `$1` lease, `$2` runner. No row for a fleet's lease or another runner's.
pub const SELECT_DRAFT_LEASE: &str = include_str!("select_draft_lease.sql");
