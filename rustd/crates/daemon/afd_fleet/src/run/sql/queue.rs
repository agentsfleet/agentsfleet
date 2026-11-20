//! Ledger discovery and fenced transitions. Lock order is slot, admission, run.

/// Eligible fleets whose oldest admission is unparked or has no live approval blocker.
pub const SELECT_CANDIDATES: &str = include_str!("run/select_ledger_candidates.sql");

/// Whether any work waits, and which probed tenant runners have none to take.
pub const SELECT_WAITING_WORK: &str = include_str!("run/select_waiting_work.sql");

/// Claim the slot and its oldest admission, and open the delivery.
///
/// One statement stamps the turn's fence and opens its event in the same
/// update, expires any prior run, reads the active fleet and its setup, and
/// returns the fleet's counters. Selecting the head before SKIP LOCKED prevents exposing a later row. The
/// runnable check follows that lock; the caller rolls the slot and admission
/// back together when the head is blocked or gone. A fleet that is not active
/// claims nothing: no fence moves and no row opens.
pub const CLAIM_HEAD: &str = include_str!("run/claim_turn_head.sql");

/// Lock the current slot before its admission; false means the claim was superseded.
pub(crate) const LOCK_TURN: &str = include_str!("run/lock_turn.sql");
/// Refusals and terminal redeliveries settle only a row with a durable ending.
pub(crate) const SETTLE_TERMINAL: &str = include_str!("run/settle_terminal.sql");
/// A separate statement after locking observes an answer that raced the park.
pub(crate) const PARK_WAITING: &str = include_str!("run/park_waiting.sql");
