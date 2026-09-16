//! Every statement that touches `core.fleet_obligations`.
//!
//! Split from the module that runs them for the reason `afd_admission::sql`
//! states for its own table: the `$n` order is written once, beside the text
//! that orders it, so nothing can run one of these with the parameters
//! transposed. The pairs here are why it matters — [`RECEIPT_DELIVERY`] and
//! [`REAPPEND_RECEIPT`] differ by one guard and take the same three
//! parameters, and [`SELECT_UNRECEIPTED`] and [`SELECT_UNDELIVERED`] differ
//! by which NULL test they ride an index on.

/// Stamp an obligation as delivered, keyed by the answer's own identity.
///
/// Keys on `(fleet_id, event_id)` rather than the obligation's row id because
/// that pair is the table's unique constraint and is already on the job the
/// poster carried — threading a row id through the queue would put a second
/// identity on the wire that has to agree with the first.
///
/// Guarded on `delivered_at IS NULL` so a redelivery cannot move a timestamp
/// that already recorded when the destination first took the answer. The path
/// is at-least-once, so this statement has to be idempotent in the same way the
/// obligation's insert is.
pub(crate) const STAMP_DELIVERED: &str = "\
UPDATE core.fleet_obligations
   SET delivered_at  = $3::bigint,
       attempt_count = attempt_count + 1,
       updated_at    = $3::bigint
 WHERE fleet_id = $1::uuid AND event_id = $2::text AND delivered_at IS NULL";

/// Obligations the queue never confirmed, oldest first.
///
/// The recovery set for a crash between the report's commit and the append that
/// follows it. `created_at < $1` leaves a row alone for its own committer for
/// [`MIN_AGE`](crate::producer::MIN_AGE), so this pass does not race a report
/// that is about to record its own receipt and put a second entry on the queue
/// for an answer already in flight.
///
/// Rides `idx_fleet_obligations_unreceipted`, whose predicate is the same NULL
/// test.
///
/// The three `::text` casts are load-bearing, not decoration: `id`, `fleet_id`
/// and `workspace_id` are `UUID` columns and [`Owed`](crate::obligation::Owed)
/// decodes them as `String`, which `sqlx` refuses to do without the cast. An
/// EMPTY scan decodes nothing and passes either way, so the uncast form failed
/// only once a row was actually owed — which is the only time this statement
/// runs. `afd_admission`'s replay scan casts the same three for the same
/// reason.
pub(crate) const SELECT_UNRECEIPTED: &str = "\
SELECT id::text, fleet_id::text, workspace_id::text, provider, event_id, answer
  FROM core.fleet_obligations
 WHERE receipt IS NULL AND created_at < $1::bigint
 ORDER BY created_at, seq
 LIMIT $2::bigint";

/// Obligations the queue carried and nobody received, oldest first per fleet.
///
/// This is the set a lost consumer group, a wholly lost stream, and a worker
/// replaced under a different hostname all leave behind — three failures the
/// queue cannot tell apart and none of which it can recover from, because in
/// every one of them the entry is simply gone while the answer is still owed.
///
/// Re-appending is safe precisely because it is not free: the destination may
/// receive the answer twice. That is the trade the whole path makes — at-least
/// once, in a thread a person reads — and it is why `delivered_at` is stamped
/// by the poster rather than by the acknowledgement, so a row only leaves this
/// set when somebody actually got it.
///
/// Rides `idx_fleet_obligations_undelivered`, leading on `fleet_id` because
/// order is promised per destination.
pub(crate) const SELECT_UNDELIVERED: &str = "\
SELECT id::text, fleet_id::text, workspace_id::text, provider, event_id, answer
  FROM core.fleet_obligations
 WHERE receipt IS NOT NULL AND delivered_at IS NULL AND updated_at < $1::bigint
 ORDER BY fleet_id, created_at, seq
 LIMIT $2::bigint";

/// Owe a delivery, in the report's own transaction.
///
/// The write that closes 7.6's window. `receipt` and `delivered_at` start NULL
/// because the queue append is NOT part of that transaction and cannot be —
/// nothing spans PostgreSQL and Dragonfly — so the append happens after and
/// records its entry id back. Until it does, this row IS the obligation.
///
/// `ON CONFLICT DO NOTHING` on the event, so a re-sent report owes one delivery
/// and not two. That agrees with the settle, which answers a repeat
/// `AlreadySettled` and charges nothing: both halves of a replayed report are
/// no-ops, which is what makes the endpoint idempotent rather than merely
/// idempotent about money. `RETURNING` therefore yields a row only when this
/// call is the one that created it, and the caller appends only what it wrote.
///
/// `$1` row id, `$2` fleet, `$3` workspace, `$4` provider, `$5` event,
/// `$6` answer, `$7` now.
pub(crate) const OWE_DELIVERY: &str = "\
INSERT INTO core.fleet_obligations
  (id, fleet_id, workspace_id, provider, event_id, answer,
   receipt, delivered_at, attempt_count, created_at, updated_at)
VALUES ($1::uuid, $2::uuid, $3::uuid, $4::text, $5::text, $6::text,
        NULL, NULL, 0, $7::bigint, $7::bigint)
ON CONFLICT ON CONSTRAINT uq_fleet_obligations_event DO NOTHING
RETURNING id";

/// Record the entry an obligation was appended to, the FIRST time.
///
/// Guarded on the receipt still being NULL so the report path cannot overwrite
/// one the producer recorded first. The loser writes nothing and its entry
/// becomes a duplicate the worker acknowledges without delivering — the safe
/// direction, since the alternative is a row pointing at an entry nobody holds.
///
/// `$1` obligation row, `$2` receipt, `$3` now.
pub(crate) const RECEIPT_DELIVERY: &str = "\
UPDATE core.fleet_obligations
   SET receipt = $2::text, updated_at = $3::bigint
 WHERE id = $1::uuid AND receipt IS NULL";

/// Record the entry an obligation was RE-appended to.
///
/// The sibling above, deliberately without its guard, and the pair is written
/// together so the one difference between them is the thing a reader sees. This
/// pass only ever reaches rows whose entry the queue lost, and the undelivered
/// scan finds rows that already HAVE a receipt — a guarded statement would write
/// nothing there and strand every row it touched.
///
/// `$1` obligation row, `$2` receipt, `$3` now.
pub(crate) const REAPPEND_RECEIPT: &str = "\
UPDATE core.fleet_obligations
   SET receipt = $2::text, updated_at = $3::bigint
 WHERE id = $1::uuid";
