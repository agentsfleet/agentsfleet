//! Every statement that touches `fleet.obligations`.
//!
//! Each is a `.sql` file beside this module, schema-qualified (RULE NSQ), and
//! checked against the rebuilt schema by `afd_db`'s
//! `test_every_sql_file_prepares`. Split from the module that runs them for the
//! reason `afd_admission::sql` states for its own table: the `$n` order is
//! written once, beside the text that orders it, so nothing can run one of
//! these with the parameters transposed. The pairs here are why it matters —
//! [`RECEIPT_DELIVERY`] and [`REAPPEND_RECEIPT`] differ by one guard and take
//! the same two parameters, and [`SELECT_UNRECEIPTED`] and
//! [`SELECT_UNDELIVERED`] differ by which NULL test they ride an index on.
//!
//! # The database owns every stamp
//!
//! `created_at`, `updated_at`, `delivered_at` and `abandoned_at` are written by
//! `now()`, and both scans measure age against `statement_timestamp()`, so
//! every instant on a row and every age compared with one comes off the same
//! clock. Rust binds the row id, which it mints, and a span; never an instant.

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
///
/// It does NOT touch `attempt_count`. It used to, and the counter that produced
/// was a count of successes wearing the name of a count of attempts: the only
/// branch that reaches this statement is the delivered one, so a destination
/// that refused an answer nine times and took it on the tenth recorded `1`, and
/// one that refused it forever recorded `0`. [`COUNT_ATTEMPT`] does the
/// counting now, at the start of the cycle, where failure is still a possible
/// ending.
///
/// `$1` fleet, `$2` event.
pub(crate) const STAMP_DELIVERED: &str = include_str!("stamp_delivered.sql");

/// Record that a worker has taken this obligation for a delivery cycle.
///
/// One cycle is one `deliver_with_retry` call — the worker accepting the job
/// and running it to a terminal verdict. The vendor retries INSIDE that call
/// are not counted separately: they are one destination's backoff schedule.
/// What this counts is how many times the queue has handed this answer to a
/// worker and not got it delivered.
///
/// Guarded on `delivered_at IS NULL`, so redelivering an answer somebody
/// already received counts nothing and matches no row. `RETURNING` therefore
/// yields the new count only when this call is the one that took the job.
///
/// `updated_at` moves with it, and that is a deliberate pacing change.
/// [`SELECT_UNDELIVERED`] re-offers answers whose rows have been untouched for
/// [`LOST_AFTER`](crate::producer::LOST_AFTER); a row a worker accepted a
/// moment ago is a row somebody is working on. A worker that DIES mid-cycle
/// leaves `updated_at` at the start of the cycle, so the window still expires
/// and the scan still re-offers it.
///
/// `$1` fleet, `$2` event.
pub(crate) const COUNT_ATTEMPT: &str = include_str!("count_attempt.sql");

/// Obligations the queue never confirmed, oldest first.
///
/// The recovery set for a crash between the report's commit and the append that
/// follows it. The age floor leaves a row alone for its own committer for
/// [`MIN_AGE`](crate::producer::MIN_AGE), so this pass does not race a report
/// that is about to record its own receipt.
///
/// Rides `idx_obligations_unreceipted`, whose predicate is the same NULL tests.
/// A row already abandoned is never offered: it was refused for good.
///
/// The three `::text` casts are load-bearing: `id`, `fleet_id` and
/// `workspace_id` are `UUID` columns and [`Owed`](crate::obligation::Owed)
/// decodes them as `String`, which `sqlx` refuses to do without the cast.
///
/// `$1` minimum age (an interval), `$2` limit.
pub(crate) const SELECT_UNRECEIPTED: &str = include_str!("select_unreceipted.sql");

/// Obligations the queue carried and nobody received, oldest first per fleet.
///
/// This is the set a lost consumer group, a wholly lost stream, and a worker
/// replaced under a different hostname all leave behind — three failures the
/// queue cannot tell apart, because in every one of them the entry is gone
/// while the answer is still owed.
///
/// Re-appending is safe precisely because it is not free: the destination may
/// receive the answer twice. `delivered_at` is stamped by the poster rather
/// than by the acknowledgement, so a row only leaves this set when somebody
/// actually got it.
///
/// Rides `idx_obligations_undelivered`, leading on `fleet_id` because order is
/// promised per destination, and skips abandoned rows as the scan above does.
///
/// `$1` how long a row must sit untouched (an interval), `$2` limit.
pub(crate) const SELECT_UNDELIVERED: &str = include_str!("select_undelivered.sql");

/// Owe a delivery, in the report's own transaction.
///
/// `receipt` and `delivered_at` start NULL because the queue append is NOT part
/// of that transaction and cannot be — nothing spans PostgreSQL and Dragonfly —
/// so the append happens after and records its entry id back. Until it does,
/// this row IS the obligation.
///
/// `ON CONFLICT DO NOTHING` on the event's unique pair, so a re-sent report owes
/// one delivery and not two. That agrees with the settle, which answers a
/// repeat `AlreadySettled` and charges nothing. `RETURNING` therefore yields a
/// row only when this call is the one that created it, and the caller appends
/// only what it wrote.
///
/// `$1` row id, `$2` fleet, `$3` workspace, `$4` provider, `$5` event,
/// `$6` answer, `$7` destination.
pub(crate) const OWE_DELIVERY: &str = include_str!("owe_delivery.sql");

/// Record the entry an obligation was appended to, the FIRST time.
///
/// Guarded on the receipt still being NULL so the report path cannot overwrite
/// one the producer recorded first. The loser writes nothing and its entry
/// becomes a duplicate the worker acknowledges without delivering — the safe
/// direction, since the alternative is a row pointing at an entry nobody holds.
///
/// `$1` obligation row, `$2` receipt.
pub(crate) const RECEIPT_DELIVERY: &str = include_str!("receipt_delivery.sql");

/// Record the entry an obligation was RE-appended to.
///
/// The sibling above, deliberately without its guard. This pass only ever
/// reaches rows whose entry the queue lost, and the undelivered scan finds rows
/// that already HAVE a receipt — a guarded statement would write nothing there
/// and strand every row it touched.
///
/// `$1` obligation row, `$2` receipt.
pub(crate) const REAPPEND_RECEIPT: &str = include_str!("reappend_receipt.sql");

/// Give up on an answer: its destination refused it for good, or it has spent
/// its delivery cycles.
///
/// Stamped before the acknowledgement, for the reason [`STAMP_DELIVERED`] is.
/// Guarded on the row being neither delivered nor already abandoned, so only
/// the one write that lands answers a row — which is what lets the caller log
/// an abandonment exactly once however many duplicate entries reach it.
///
/// `$1` fleet, `$2` event, `$3` reason.
pub(crate) const ABANDON: &str = include_str!("abandon.sql");
