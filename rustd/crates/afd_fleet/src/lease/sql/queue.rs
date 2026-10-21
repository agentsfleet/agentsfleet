//! Ledger discovery and fenced transitions. Lock order is slot, admission, lease.

/// Eligible fleets with an unparked oldest admission, preferring held fleets and recent hints.
pub const SELECT_CANDIDATES: &str = include_str!("lease/select_ledger_candidates.sql");

/// Claim the slot, stamp its exact oldest admission and expire any prior lease.
///
/// Selecting the head before SKIP LOCKED prevents exposing a later row. The
/// caller rolls the transaction back when no admission is returned.
pub const CLAIM_HEAD: &str = include_str!("lease/claim_admission_head.sql");

/// Lock the current slot before its admission; false means the claim was superseded.
pub(crate) const LOCK_ADMISSION: &str = include_str!("lease/lock_admission.sql");
/// Delivery and narrative insertion share one transaction under that lock.
pub(crate) const STAMP_DELIVERY: &str = include_str!("lease/stamp_delivery.sql");
/// Refusals and terminal redeliveries settle only a row with a durable ending.
pub(crate) const SETTLE_TERMINAL: &str = include_str!("lease/settle_terminal.sql");
/// A separate statement after locking observes an answer that raced the park.
pub(crate) const PARK_WAITING: &str = include_str!("lease/park_waiting.sql");
