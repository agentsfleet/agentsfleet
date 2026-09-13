//! `core.fleet_admissions` — every statement that touches the table.
//!
//! Private to the crate with one exception, so nothing outside it can run a
//! statement with the parameters in another order. The `$n` order is written
//! once, here, beside the text it orders.
//!
//! [`MARK_DELIVERED`] is public because the lease path runs it on the
//! connection that just opened the narrative log, and a method here would take
//! a second connection from the pool to write one column. That is the shape
//! `afd_events::sql` already uses for the same reason: the table's owner keeps
//! the text, the writer keeps its `bind` chain.

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

/// Stamp the instant a runner was handed this event.
///
/// Run by the lease path on the same connection as its
/// `afd_events::sql::INSERT_FLEET_EVENT`, and on BOTH of that insert's arms. A
/// first delivery that committed the narrative row and then failed to stamp
/// takes the conflict arm on redelivery; stamping only on the first arm would
/// leave that row unstamped forever, and [`SELECT_UNDELIVERED_FLEETS`] would
/// eventually read it as accepted work whose entry is gone and re-append an
/// event that already ran. The `delivered_at IS NULL` guard below is what
/// makes the second attempt free.
///
/// Keyed on the fleet and the logical event id's two integers, NOT on the
/// receipt. A replayed admission put one logical event on two stream entries
/// while this table recorded only the first receipt, so a receipt-keyed stamp
/// would miss the delivery of the second entry, leave the row unstamped
/// forever, and have [`SELECT_UNDELIVERED_FLEETS`] offer it up for recovery
/// every pass. `core.fleet_events` dedups on the logical id for the same
/// reason, so the two agree by construction.
///
/// Guarded on the stamp still being absent, which keeps the write off the
/// index for a row already stamped and makes a double delivery idempotent.
///
/// `$1` fleet, `$2` the logical id's `created_at`, `$3` its `seq`, `$4` now.
pub const MARK_DELIVERED: &str = "\
UPDATE core.fleet_admissions
SET delivered_at = $4, updated_at = $4
WHERE fleet_id = $1::uuid AND created_at = $2 AND seq = $3
  AND delivered_at IS NULL";

/// One fleet per row, with its oldest admission that is receipted and not
/// delivered — the probe the reconciliation pass starts from.
///
/// The steady state is what this shape is for. A fleet with undelivered work is
/// ordinarily just a fleet whose runner has not got to it yet, and asking the
/// stream about every such row every pass would be round trips spent proving
/// nothing. One question per fleet answers it: if the stream still holds that
/// fleet's OLDEST undelivered receipt, its data is there and the pass moves on.
/// Only a fleet that answers no pays for a row-by-row walk.
///
/// `DISTINCT ON` rides `idx_fleet_admissions_undelivered` — the index's leading
/// column is `fleet_id` and its order is the `ORDER BY` — so an idle
/// deployment reads an empty index and a busy one reads one entry per fleet.
///
/// No `FOR UPDATE`: this statement decides only which streams to ASK about, and
/// locking a row here would make the probe hold a transaction open across a
/// network round trip to the datastore. [`SELECT_UNDELIVERED_ON_FLEET`] takes
/// the locks, on the fleet that needs them.
///
/// `$1` how many fleets one pass may examine.
pub(crate) const SELECT_UNDELIVERED_FLEETS: &str = "\
SELECT DISTINCT ON (fleet_id) fleet_id::text, receipt
FROM core.fleet_admissions
WHERE receipt IS NOT NULL AND delivered_at IS NULL
ORDER BY fleet_id, created_at, seq
LIMIT $1";

/// Every admission on one fleet that is receipted and not delivered, locked
/// for this pass.
///
/// Read only for a fleet whose oldest receipt the stream could not produce —
/// its data is gone, and each of these rows has to be asked about in turn
/// because a rebuilt stream may already hold NEW entries that are perfectly
/// alive.
///
/// `FOR UPDATE SKIP LOCKED` for the reason the replay scan takes it: every
/// replica runs the sweeper, and two passes must take disjoint rows rather
/// than both voiding one.
///
/// `$1` fleet, `$2` the batch limit.
pub(crate) const SELECT_UNDELIVERED_ON_FLEET: &str = "\
SELECT id::text, receipt
FROM core.fleet_admissions
WHERE fleet_id = $1::uuid AND receipt IS NOT NULL AND delivered_at IS NULL
ORDER BY created_at, seq
LIMIT $2
FOR UPDATE SKIP LOCKED";

/// Forget a receipt whose entry the datastore no longer holds.
///
/// The whole of the repair: the row goes back to `receipt IS NULL`, which is
/// the state [`SELECT_UNRECEIPTED`] already scans, so the replay sweeper
/// re-appends it and marks the fleet ready with no second append path to keep
/// correct. It also re-enters the deployment's replay backlog, which is honest
/// — the work IS owed again.
///
/// Guarded on the row still being receipted and still undelivered, so a
/// delivery that landed between the probe and this write keeps its receipt.
///
/// `$1` id, `$2` now.
pub(crate) const VOID_LOST_RECEIPT: &str = "\
UPDATE core.fleet_admissions
SET receipt = NULL, updated_at = $2
WHERE id = $1::uuid AND receipt IS NOT NULL AND delivered_at IS NULL";
