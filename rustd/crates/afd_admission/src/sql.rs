//! `core.fleet_admissions` — every statement that touches the table.
//!
//! Private to the crate: only the ledger binds these, so nothing outside it
//! can run one with the parameters in another order. The `$n` order is
//! written once, here, beside the text it orders.

/// Commit an admission, or find the one an earlier call committed.
///
/// `ON CONFLICT DO UPDATE` rather than `DO NOTHING`, for two reasons that
/// are the same reason: an update is what makes `RETURNING` answer on the
/// conflict arm, so a retry learns its id in ONE round trip; and an update
/// takes the row lock, so two daemons admitting one key at the same instant
/// are serialised by Postgres — the second waits for the first's commit and
/// then reads the row it wrote. `xmax = 0` is how the two are told apart:
/// a freshly inserted row has no updating transaction, a conflicted one does.
///
/// `$1` id, `$2` fleet, `$3` workspace, `$4` producer, `$5` producer key,
/// `$6` payload digest, `$7` actor, `$8` event type, `$9` body, `$10` now,
/// `$11` the initial replay count.
pub(crate) const INSERT_ADMISSION: &str = "\
INSERT INTO core.fleet_admissions
  (id, fleet_id, workspace_id, producer, producer_key, payload_digest,
   actor, event_type, request_json, event_created_at, replay_count,
   created_at, updated_at)
VALUES ($1::uuid, $2::uuid, $3::uuid, $4, $5, $6, $7, $8, $9, $10, $11, $10, $10)
ON CONFLICT (producer, producer_key) DO UPDATE SET updated_at = EXCLUDED.updated_at
RETURNING (xmax = 0) AS inserted, created_at, seq, receipt, payload_digest";

/// Record the receipt the queue answered an admission's append with.
///
/// Guarded on the receipt still being absent: a row the replay sweeper got
/// to first keeps the sweeper's receipt, and the caller learns from a zero
/// row count that its own entry is the extra one.
///
/// `$1` id, `$2` receipt, `$3` now.
pub(crate) const RECORD_RECEIPT: &str = "\
UPDATE core.fleet_admissions
SET receipt = $2, updated_at = $3
WHERE id = $1::uuid AND receipt IS NULL";

/// The admitted rows that never got a receipt, oldest first, locked for this
/// pass.
///
/// `FOR UPDATE SKIP LOCKED` because every replica runs the sweeper:
/// concurrent passes take disjoint batches instead of both re-appending one
/// row. The age cutoff keeps a pass from racing an admission still in
/// flight — an inserter that is about to append — and rides the partial
/// index on `receipt IS NULL`.
///
/// `$1` the age cutoff, `$2` the batch limit.
pub(crate) const SELECT_UNRECEIPTED: &str = "\
SELECT id::text, fleet_id::text, workspace_id::text, actor, event_type,
       request_json, event_created_at, created_at, seq
FROM core.fleet_admissions
WHERE receipt IS NULL AND created_at <= $1
ORDER BY created_at ASC, seq ASC
LIMIT $2
FOR UPDATE SKIP LOCKED";

/// Record the receipt a replay's append answered with.
///
/// The row is locked by [`SELECT_UNRECEIPTED`] in the same transaction, so
/// no guard on the receipt is needed for correctness; it is kept so a row
/// that somehow carries one is left alone rather than overwritten.
///
/// Returns the row's replay count AFTER the increment, so the sweeper can say
/// which rows keep coming back — a row on its fifth pass is poison, not
/// throughput, and the count is the only place that shows it.
///
/// `$1` id, `$2` receipt, `$3` now.
pub(crate) const RECORD_REPLAY_RECEIPT: &str = "\
UPDATE core.fleet_admissions
SET receipt = $2, replay_count = replay_count + 1, updated_at = $3
WHERE id = $1::uuid AND receipt IS NULL
RETURNING replay_count";
