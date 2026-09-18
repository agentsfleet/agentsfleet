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
/// while the estimate in `$13` is under the budget in `$12`, OR when this
/// producer key already has a row, so a retry of admitted work is answered
/// its id however deep the backlog is. Answering no row is the refusal, and
/// it costs no second round trip on the path every producer takes.
///
/// `$13` arrives already measured — see `Admissions::deployment_estimate` and
/// the note in `budget/ceiling.rs`. It used to be a `count(*)` over
/// `receipt IS NULL` right here, which read well and cost a walk of every
/// waiting row on every accepted event: the partial index keeps that walk off
/// the table but not off the rows, so the price rose with the backlog and
/// peaked when the deployment was already behind. Two bound integers compare
/// in the planner instead, and the `EXISTS` arm below is a point lookup on
/// the unique key that only runs when the estimate has already refused.
///
/// `$1` id, `$2` fleet, `$3` workspace, `$4` producer, `$5` producer key,
/// `$6` payload digest, `$7` actor, `$8` event type, `$9` body, `$10` now,
/// `$11` the initial replay count, `$12` the replay-backlog budget,
/// `$13` the estimated rows awaiting a receipt.
pub(crate) const INSERT_ADMISSION: &str = "\
INSERT INTO core.fleet_admissions
  (id, fleet_id, workspace_id, producer, producer_key, payload_digest,
   actor, event_type, request_json, event_created_at, replay_count,
   created_at, updated_at)
SELECT $1::uuid, $2::uuid, $3::uuid, $4, $5, $6, $7, $8, $9, $10, $11, $10, $10
WHERE $13::bigint < $12
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
/// network round trip to the datastore. [`SELECT_UNDELIVERED_ON_FLEET`] reads
/// under the same rule, for the same reason — it probes per row, so it would
/// hold the lock across one round trip per row rather than one per pass.
///
/// # The cursor is what makes the limit a pace instead of a ceiling
///
/// `fleet_id > $2` is a keyset bound on the same column the statement already
/// orders by, so it costs an index position rather than a scan and a skip.
/// Without it the limit reads the same lowest-sorting fleets on every pass, and
/// a deployment with more unfinished fleets than the limit never examines the
/// rest — the caller's note in
/// [`Progress`](crate::reconcile::progress::Progress) has the failure in full.
/// The bound is always present: a first pass binds
/// [`FIRST_FLEET`](crate::reconcile::progress::FIRST_FLEET), which is a true
/// lower bound on the column rather than a stand-in for its absence, so this
/// statement has no second spelling to drift from.
///
/// `$1` how many fleets one pass may examine, `$2` the fleet to resume after.
pub(crate) const SELECT_UNDELIVERED_FLEETS: &str = "\
SELECT DISTINCT ON (fleet_id) fleet_id::text, receipt
FROM core.fleet_admissions
WHERE receipt IS NOT NULL AND delivered_at IS NULL AND fleet_id > $2::uuid
ORDER BY fleet_id, created_at, seq
LIMIT $1";

/// Every admission on one fleet that is receipted and not delivered.
///
/// Read only for a fleet whose oldest receipt the stream could not produce —
/// its data is gone, and each of these rows has to be asked about in turn
/// because a rebuilt stream may already hold NEW entries that are perfectly
/// alive.
///
/// A plain read, where the replay scan takes `FOR UPDATE SKIP LOCKED`, and the
/// difference is what happens between the read and the write. Replay reads its
/// batch and re-appends inside one transaction with no other system in it. This
/// pass asks the datastore about every row it read, so a lock taken here would
/// be held across a round trip per row — and the rows it holds are the ones a
/// live producer recording its own receipt waits behind. [`VOID_LOST_RECEIPT`]
/// carries the guarantee instead, by pinning the receipt it was told about.
///
/// The cost is that two replicas walking one lost fleet probe the same rows.
/// Duplicated round trips, not a duplicated repair: the guard means one write
/// lands and the other matches nothing.
///
/// # Why this one needs a cursor too
///
/// The row comparison resumes a walk that filled its batch, and the alternative
/// is not "recovery takes longer" — it is rows that are never reached. A repair
/// VOIDS the receipts it read, the replay sweeper re-appends those rows with
/// live receipts, and they keep their place in this order because `created_at`
/// and `seq` belong to the admission rather than to the entry. So the next walk
/// re-reads the rows it just fixed, finds them healthy, and fills its batch
/// with them. `(created_at, seq) > ($3, $4)` is a row-wise comparison on the
/// leading columns of the same index, so resuming is an index bound and not a
/// scan. A first walk binds [`RowKey::FIRST`](crate::reconcile::progress::RowKey::FIRST),
/// which every admission sorts above.
///
/// `$1` fleet, `$2` the batch limit, `$3` and `$4` the row to resume after.
pub(crate) const SELECT_UNDELIVERED_ON_FLEET: &str = "\
SELECT id::text, receipt, created_at, seq
FROM core.fleet_admissions
WHERE fleet_id = $1::uuid AND receipt IS NOT NULL AND delivered_at IS NULL
  AND (created_at, seq) > ($3::bigint, $4::bigint)
ORDER BY created_at, seq
LIMIT $2";

/// Forget a receipt whose entry the datastore no longer holds.
///
/// The whole of the repair: the row goes back to `receipt IS NULL`, which is
/// the state [`SELECT_UNRECEIPTED`] already scans, so the replay sweeper
/// re-appends it and marks the fleet ready with no second append path to keep
/// correct. It also re-enters the deployment's replay backlog, which is honest
/// — the work IS owed again.
///
/// Guarded on the row still carrying THE receipt the caller probed and still
/// being undelivered, which is what lets the read above go unlocked. `receipt
/// IS NOT NULL` would not do: between the probe and this write the replay
/// sweeper can re-append the row and record a DIFFERENT receipt, and a test
/// for mere presence would then forget a receipt nobody ever asked the stream
/// about. Pinning the value makes the pair a compare-and-set — the write lands
/// only if the row is as it was when the answer was obtained — and a row that
/// moved reports zero rows affected, which the caller counts as the repair it
/// did not do.
///
/// `$1` id, `$2` now, `$3` the receipt the probe was answered for.
pub(crate) const VOID_LOST_RECEIPT: &str = "\
UPDATE core.fleet_admissions
SET receipt = NULL, updated_at = $2
WHERE id = $1::uuid AND receipt = $3::text AND delivered_at IS NULL";
