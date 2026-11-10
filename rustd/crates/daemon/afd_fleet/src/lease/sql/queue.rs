//! Ledger discovery and fenced transitions. Lock order is slot, admission, lease.

/// Eligible fleets whose oldest admission is unparked or has no live approval blocker.
pub const SELECT_CANDIDATES: &str = include_str!("lease/select_ledger_candidates.sql");

/// Claim the slot and its oldest admission, and open the delivery.
///
/// One statement stamps the admission, expires any prior lease, reads the
/// active fleet and its setup, opens the `fleet.events` row, stamps
/// `delivered_at` and returns the fleet's counters. Selecting the head before SKIP LOCKED prevents exposing a later row. The
/// runnable check follows that lock; the caller rolls the slot and admission
/// back together when the head is blocked or gone. A fleet that is not active
/// claims nothing: no fence moves and no row opens.
pub const CLAIM_HEAD: &str = include_str!("lease/claim_admission_head.sql");

/// Lock the current slot before its admission; false means the claim was superseded.
pub(crate) const LOCK_ADMISSION: &str = include_str!("lease/lock_admission.sql");
/// Refusals and terminal redeliveries settle only a row with a durable ending.
pub(crate) const SETTLE_TERMINAL: &str = include_str!("lease/settle_terminal.sql");
/// A separate statement after locking observes an answer that raced the park.
pub(crate) const PARK_WAITING: &str = include_str!("lease/park_waiting.sql");
