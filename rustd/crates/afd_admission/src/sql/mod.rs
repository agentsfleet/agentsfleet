//! `fleet.admissions` — every statement that touches the table.
//!
//! Each is a `.sql` file beside this module, schema-qualified (RULE NSQ), and
//! checked against the rebuilt schema by `afd_db`'s
//! `test_every_sql_file_prepares`. Private to the crate with two exceptions,
//! so nothing outside it can run a statement with the parameters in another
//! order. The `$n` order is written once, here, beside the text it orders.
//!
//! [`MARK_DELIVERED`] and [`SELECT_REPLY_DESTINATION`] are public because the
//! lease path runs each on a connection it already holds, and a method here
//! would take a second one from the pool to touch two columns. That is the
//! shape `afd_events::sql` already uses for the same reason: the table's owner
//! keeps the text, the caller keeps its `bind` chain.
//!
//! # The database owns every stamp
//!
//! `created_at`, `updated_at`, `event_created_at` and `delivered_at` are
//! written by `now()`, and the replay and backlog reads measure age against
//! `statement_timestamp()`, so every instant on a row and every age compared
//! with one comes off the same clock. Rust binds an instant only where it mints
//! an identifier — the row id, minted before the insert so a producer key that
//! is the row itself can be bound in the same statement.
//!
//! # The event id is the row id
//!
//! An admitted event is named by its row's `UUIDv7` everywhere it travels: the
//! stream entry, `fleet.events.event_id`, the usage ledger. The lookups the
//! lease path makes back into this table are therefore primary-key lookups,
//! scoped by the fleet so an id from another fleet answers no row.

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
/// while the estimate in `$12` is under the budget in `$11`, OR when this
/// producer key already has a row, so a retry of admitted work is answered
/// its id however deep the backlog is. Answering no row is the refusal, and
/// it costs no second round trip on the path every producer takes.
///
/// `$12` arrives already measured — see `Admissions::deployment_estimate` and
/// the note in `budget/ceiling/mod.rs`. Two bound integers compare in the
/// planner, and the `EXISTS` arm is a point lookup on the unique key that only
/// runs when the estimate has already refused.
///
/// The destination rides the same statement. A stated one arrives in `$13` and
/// `$14`; an inherited one names its event's row in `$15`, and the join copies
/// that row's pair — both halves or neither, which
/// `ck_admissions_reply_both_or_neither` then holds — so a continuation cannot
/// read a destination and lose a race to the write. The caller's
/// [`Reply`](crate::Reply) sets at most one of the two, so `COALESCE` never
/// mixes them. On the conflict arm nothing but `updated_at` moves: a retry keeps
/// the first call's destination, as it keeps its payload.
///
/// `$1` id, `$2` fleet, `$3` workspace, `$4` producer, `$5` producer key,
/// `$6` payload digest, `$7` actor, `$8` event type, `$9` body,
/// `$10` the initial replay count, `$11` the replay-backlog budget,
/// `$12` the estimated rows awaiting a receipt, `$13` a stated connector,
/// `$14` a stated address, `$15` the inherited event's row id.
///
/// It returns the row's own id, digest and fleet — the first call's, on the
/// conflict arm — so a producer that must compare a repeat's payload reads it
/// from this statement rather than a second one a deleted row could answer
/// empty.
pub(crate) const INSERT_ADMISSION: &str = include_str!("insert_admission.sql");

/// The destination one event was admitted with, by its row id — the read the
/// report makes inside its own transaction.
///
/// Public for the reason [`MARK_DELIVERED`] is. A row without a destination
/// answers no row, which is the same answer as none recorded.
///
/// `$1` fleet, `$2` the event's admission row id.
pub const SELECT_REPLY_DESTINATION: &str = include_str!("select_reply_destination.sql");

/// Record the receipt the queue answered an admission's append with.
///
/// Guarded on the receipt still being absent: a row the replay sweeper got
/// to first keeps the sweeper's receipt, and the caller learns from a zero
/// row count that its own entry is the extra one.
///
/// `$1` id, `$2` receipt.
pub(crate) const RECORD_RECEIPT: &str = include_str!("record_receipt.sql");

/// One producer key, locked without waiting behind a sibling confirmation.
///
/// `$1` producer, `$2` producer key.
pub(crate) const CONFIRM_ADMISSION: &str = include_str!("confirm_admission.sql");

/// The admitted rows that never got a receipt, oldest first, locked for this
/// pass.
///
/// `FOR UPDATE SKIP LOCKED` because every replica runs the sweeper:
/// concurrent passes take disjoint batches instead of both re-appending one
/// row. The age cutoff keeps a pass from racing an admission still in
/// flight — an inserter that is about to append — and rides the partial
/// index on `receipt IS NULL`. It is measured on the database's clock, the one
/// `created_at` was stamped by.
///
/// `$1` the minimum age, as an interval, `$2` the batch limit.
pub(crate) const SELECT_UNRECEIPTED: &str = include_str!("select_unreceipted.sql");

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
/// `$1` id, `$2` receipt.
pub(crate) const RECORD_REPLAY_RECEIPT: &str = include_str!("record_replay_receipt.sql");

/// How many rows await a receipt, and how long the oldest has waited.
///
/// Both off the partial index, so an idle deployment answers from a few
/// pages and a backed-up one from exactly the rows that are backed up. The age
/// is the database's own subtraction, so it never mixes two clocks.
pub(crate) const SELECT_BACKLOG: &str = include_str!("select_backlog.sql");

/// The newest receipt among this fleet's admissions that were DELIVERED —
/// the position a lost consumer group is recreated at.
///
/// Delivered means `fleet.events` holds the event id, which the lease path
/// writes on delivery and never deletes. Ordered by the receipt's two integers
/// rather than its text, because a receipt is a stream entry id and `999-0`
/// sorts after `1000-0` as text; a cursor one decade off would re-run or skip a
/// thousand entries. No row means nothing on the stream was ever delivered.
///
/// `$1` fleet.
pub(crate) const SELECT_DELIVERED_CURSOR: &str = include_str!("select_delivered_cursor.sql");

/// Stamp the instant a runner was handed this event.
///
/// Run by the lease path on the same connection as its
/// `afd_events::sql::INSERT_FLEET_EVENT`, and on BOTH of that insert's arms. A
/// first delivery that committed the narrative row and then failed to stamp
/// takes the conflict arm on redelivery; stamping only on the first arm would
/// leave that row unstamped forever, and [`SELECT_UNDELIVERED_FLEETS`] would
/// eventually read it as accepted work whose entry is gone and re-append an
/// event that already ran. The `delivered_at IS NULL` guard is what makes the
/// second attempt free.
///
/// Keyed on the event's row id, NOT on the receipt. A replayed admission put
/// one event on two stream entries while this table recorded only the first
/// receipt, so a receipt-keyed stamp would miss the delivery of the second
/// entry, leave the row unstamped forever, and have
/// [`SELECT_UNDELIVERED_FLEETS`] offer it up for recovery every pass.
/// `fleet.events` dedups on the event id for the same reason, so the two agree
/// by construction.
///
/// `$1` fleet, `$2` the event's admission row id.
pub const MARK_DELIVERED: &str = include_str!("mark_delivered.sql");

/// One fleet per row, with its oldest admission that is receipted and not
/// delivered — the probe the reconciliation pass starts from.
///
/// A fleet with undelivered work is ordinarily just a fleet whose runner has
/// not got to it yet, and asking the stream about every such row every pass
/// would be round trips spent proving nothing. One question per fleet answers
/// it: if the stream still holds that fleet's OLDEST undelivered receipt, its
/// data is there and the pass moves on. Only a fleet that answers no pays for
/// a row-by-row walk.
///
/// No `FOR UPDATE`: this statement decides only which streams to ASK about, and
/// locking a row here would make the probe hold a transaction open across a
/// network round trip to the datastore.
///
/// # The cursor is what makes the limit a pace instead of a ceiling
///
/// `fleet_id > $2` is a keyset bound on the column the statement orders by.
/// Without it the limit reads the same lowest-sorting fleets on every pass, and
/// a deployment with more unfinished fleets than the limit never examines the
/// rest. The bound is always present: a first pass binds
/// [`FIRST_FLEET`](crate::reconcile::progress::FIRST_FLEET), a true lower bound
/// on the column rather than a stand-in for its absence.
///
/// `$1` how many fleets one pass may examine, `$2` the fleet to resume after.
pub(crate) const SELECT_UNDELIVERED_FLEETS: &str = include_str!("select_undelivered_fleets.sql");

/// Every admission on one fleet that is receipted and not delivered.
///
/// Read only for a fleet whose oldest receipt the stream could not produce —
/// its data is gone, and each of these rows has to be asked about in turn
/// because a rebuilt stream may already hold NEW entries that are alive.
///
/// A plain read, where the replay scan takes `FOR UPDATE SKIP LOCKED`: this
/// pass asks the datastore about every row it read, so a lock taken here would
/// be held across a round trip per row. [`VOID_LOST_RECEIPT`] carries the
/// guarantee instead, by pinning the receipt it was told about.
///
/// # Why this one needs a cursor too
///
/// A repair VOIDS the receipts it read, the replay sweeper re-appends those
/// rows with live receipts, and they keep their place in this order because
/// `created_at` and `sequence` belong to the admission rather than to the
/// entry. So the next walk would re-read the rows it just fixed and fill its
/// batch with them. `(created_at, sequence) > ($3, $4)` is a row-wise
/// comparison on the leading columns of the same index, so resuming is an
/// index bound and not a scan. A first walk binds
/// [`RowKey::FIRST`](crate::reconcile::progress::RowKey::FIRST), which every
/// admission sorts above.
///
/// `$1` fleet, `$2` the batch limit, `$3` and `$4` the row to resume after.
pub(crate) const SELECT_UNDELIVERED_ON_FLEET: &str =
    include_str!("select_undelivered_on_fleet.sql");

/// Forget a receipt whose entry the datastore no longer holds.
///
/// The whole of the repair: the row goes back to `receipt IS NULL`, the state
/// [`SELECT_UNRECEIPTED`] already scans, so the replay sweeper re-appends it
/// and marks the fleet ready with no second append path to keep correct.
///
/// Guarded on the row still carrying THE receipt the caller probed and still
/// being undelivered, which is what lets the read above go unlocked. Between
/// the probe and this write the replay sweeper can re-append the row and record
/// a DIFFERENT receipt; pinning the value makes the pair a compare-and-set, and
/// a row that moved reports zero rows affected.
///
/// `$1` id, `$2` the receipt the probe was answered for.
pub(crate) const VOID_LOST_RECEIPT: &str = include_str!("void_lost_receipt.sql");

/// The admission a producer already recorded under its key, if any: its id,
/// the digest it was admitted with, its fleet and when it was admitted.
///
/// Read when a spent fleet budget or the caller's own check refused before the
/// insert, because a key already admitted is work already accepted and refusing
/// its retry would tell the sender a message it owns never landed. Rides
/// `uq_admissions_producer_producer_key`.
///
/// `$1` producer, `$2` producer key.
pub(crate) const SELECT_REPEATED: &str = include_str!("select_repeated.sql");

/// Whether this fleet already holds a row under the producer's key — a repeat
/// skips the policy check, because its admission was checked the first time.
///
/// `$1` fleet, `$2` workspace, `$3` producer, `$4` producer key.
pub(crate) const SELECT_REPEATED_ON_FLEET: &str = include_str!("select_repeated_on_fleet.sql");

/// The policy semantics the fleet's head revision was written under, holding
/// the fleet row against a concurrent setup change until the admission commits.
///
/// `$1` fleet, `$2` workspace.
pub(crate) const SELECT_INSTALLED_POLICY: &str = include_str!("select_installed_policy.sql");
