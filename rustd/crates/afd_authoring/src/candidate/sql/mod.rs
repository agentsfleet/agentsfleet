//! Activity receipts are content-free; a proposal's revision is its candidate's.

/// An activity's receipt, with the revision of the candidate it proposed while
/// that candidate exists.
///
/// `$1` attempt, `$2` activity sequence.
pub(super) const RECEIPT: &str = include_str!("receipt.sql");

/// `$1` id, `$2` attempt, `$3` activity sequence, `$4` request digest,
/// `$5` the proposed candidate.
pub(super) const INSERT_RECEIPT: &str = include_str!("insert_receipt.sql");

/// `$1` attempt, `$2` the accepted activity sequence.
pub(super) const ADVANCE: &str = include_str!("advance.sql");
