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
/// The deployment budget rides the same statement. The row is inserted only
/// while fewer than `$12` rows await a receipt — a count the partial index
/// on `receipt IS NULL` answers without touching the table — OR when this
/// producer key already has a row, so a retry of admitted work is answered
/// its id however deep the backlog is. Answering no row is the refusal, and
/// it costs no second round trip on the path every producer takes.
///
/// `$1` id, `$2` fleet, `$3` workspace, `$4` producer, `$5` producer key,
/// `$6` payload digest, `$7` actor, `$8` event type, `$9` body, `$10` now,
/// `$11` the initial replay count, `$12` the replay-backlog budget.
pub(crate) const INSERT_ADMISSION: &str = "\
INSERT INTO core.fleet_admissions
  (id, fleet_id, workspace_id, producer, producer_key, payload_digest,
   actor, event_type, request_json, event_created_at, replay_count,
   created_at, updated_at)
SELECT $1::uuid, $2::uuid, $3::uuid, $4, $5, $6, $7, $8, $9, $10, $11, $10, $10
WHERE (SELECT count(*) FROM core.fleet_admissions WHERE receipt IS NULL) < $12
   OR EXISTS (SELECT 1 FROM core.fleet_admissions
              WHERE producer = $4 AND producer_key = $5)
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

/// How many rows await a receipt, and when the oldest was admitted.
///
/// Both off the partial index, so an idle deployment answers from a few
/// pages and a backed-up one from exactly the rows that are backed up.
pub(crate) const SELECT_BACKLOG: &str = "\
SELECT count(*), min(created_at)
FROM core.fleet_admissions
WHERE receipt IS NULL";

/// The newest receipt among this fleet's admissions that were DELIVERED —
/// the position a lost consumer group is recreated at.
///
/// Delivered means `core.fleet_events` holds the logical id, which the lease
/// path writes on delivery and never deletes. Ordered by the receipt's two
/// integers rather than its text, because `999-0` sorts after `1000-0` as
/// text and a cursor one decade off would re-run or skip a thousand entries.
/// No row means nothing on the stream was ever delivered.
///
/// `$1` fleet.
pub(crate) const SELECT_DELIVERED_CURSOR: &str = "\
SELECT a.receipt
FROM core.fleet_admissions a
JOIN core.fleet_events e
  ON e.fleet_id = a.fleet_id
 AND e.event_id = a.created_at::text || '-' || a.seq::text
WHERE a.fleet_id = $1::uuid AND a.receipt IS NOT NULL
ORDER BY split_part(a.receipt, '-', 1)::bigint DESC,
         split_part(a.receipt, '-', 2)::bigint DESC
LIMIT 1";
