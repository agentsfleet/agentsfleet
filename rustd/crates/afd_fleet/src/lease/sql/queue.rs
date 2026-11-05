//! The ledger claim and its fenced endings. Lock order is slot, admission, lease.

/// Claim the oldest ready slot this runner may take, stamp its head admission
/// and open its event row, or replay a repeated poll key — one statement.
///
/// The slot is locked first, driven off the partial readiness index: each
/// ready slot's fleet and head admission are found by a keyed lateral lookup,
/// and the first slot in claim order is locked with `SKIP LOCKED`. Only then is
/// its head locked, by primary key alone, and the parked and settled marks are
/// read from the locked row, which is the latest version. A head joined into
/// the slot's lock could be reached through the unsettled index, and on stale
/// statistics the planner read every unsettled admission in the ledger to find
/// one. A head held elsewhere, or moved since the statement began, leaves this
/// poll empty. Every write joins the pick, so a claim that finds nothing changes
/// no row. The clock is read once, after the locks. `$1` runner, `$2` the fleets whose sandboxes it holds, `$3` the lease
/// lifetime, `$4` how long a silent runner stays live, `$5` the runner status
/// that may lease, `$6` and `$7` the active and expired lease statuses, `$8`
/// the fleet status that may run, `$9` the received event status, `$10` the
/// steer producer, `$11` the poll's key or NULL.
pub const CLAIM_HEAD: &str = include_str!("lease/claim_admission_head.sql");

/// Lock the current slot before its admission; false means the claim was superseded.
pub(crate) const LOCK_ADMISSION: &str = include_str!("lease/lock_admission.sql");
/// Settle a redelivered event that already ended, and free the slot, under the
/// claim's fence. `$1` fleet, `$2` event, `$3` fence.
pub(crate) const SETTLE_TERMINAL: &str = include_str!("lease/settle_terminal.sql");
/// End a refused event, settle its admission and free the slot, in one
/// statement fenced on the claim. A superseded fence writes nothing.
///
/// Guarded on `status = $6` — always `received` — so a terminal row is never
/// reopened; no row back means the event had already ended, which happens when
/// a refused delivery's acknowledgement was lost. The closing carries the
/// terminal row and the fleet facts the tail's completion frame publishes.
/// `NULLIF($7, '')` keeps an empty detail `NULL`.
///
/// `$1` fleet, `$2` event, `$3` fence, `$4` the terminal status, `$5` the
/// failure label, `$6` the status the event must still hold, `$7` detail, `$8`
/// the gate status that counts as pending.
pub(crate) const END_REFUSED: &str = include_str!("lease/end_refused.sql");
/// Lock, in fleet order, the slot of every fleet whose parked head no card
/// holds any more: no standing card pending, and every pending card on the
/// head past its deadline. The first statement of the lapsed-park sweep, so
/// the unpark after it takes no admission before its slot. `SKIP LOCKED`: a
/// slot another writer holds is left for the next pass, never waited on.
pub(crate) const LOCK_LAPSED_PARK_SLOTS: &str = include_str!("lease/lock_lapsed_park_slots.sql");
/// Unpark the heads of the fleets `$1` whose slots are held, re-checking the
/// rule on a snapshot taken after the locks. The cards stay pending: a person
/// may still answer one late, and only an answer resolves standing access.
pub(crate) const UNPARK_LAPSED_HEADS: &str = include_str!("lease/unpark_lapsed_heads.sql");
/// Bound a sweep's transaction: `$1` lock timeout, `$2` statement timeout,
/// both local to the transaction.
pub(crate) const BOUND_TRANSACTION: &str = include_str!("lease/bound_transaction.sql");
/// A separate statement after locking observes an answer that raced the park.
pub(crate) const PARK_WAITING: &str = include_str!("lease/park_waiting.sql");
