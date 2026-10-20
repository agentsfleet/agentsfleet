//! The statements the background sweepers run.
//!
//! They live in their own module rather than beside the runner store's for the
//! reason [`super`] gives: the split is by CALLER, and nothing on a request
//! path runs any of these.
//!
//! # Every one of them is bounded and idempotent
//!
//! A sweeper's pass can fail halfway and be re-run, so each statement either
//! changes nothing on a second pass (the offline event's unique stamp, the
//! drained transition's status predicate) or converges (the slot expiry's `>`
//! guard). That is what lets [`crate::sweep::run`] report a failure and carry
//! on rather than having to unwind one.
//!
//! # Every one of them reads the database's clock
//!
//! None takes a lock it then judges an expiry under, so each compares against
//! `statement_timestamp()` — the clock an index can bound, and the same instant
//! for every row one statement visits. Spans bind as intervals.

/// The runners one liveness pass considers.
///
/// Three disjunctions, and each is a different reason a runner needs looking
/// at: it has gone quiet, it is draining, or it is not active and still holds
/// leases. A runner that is none of these is healthy and is not fetched at all
/// — which is what keeps the pass proportional to the trouble rather than to
/// the fleet. `checked_at` is the instant the staleness test used, so the pass
/// judges each row against the clock that selected it.
///
/// `ORDER BY r.updated_at ASC, r.id ASC` with a LIMIT is the pass's bound: the
/// least recently touched runners are swept first, so no runner can be starved
/// by a busier neighbour, and `id` settles a same-millisecond tie so the order
/// is total.
///
/// `$1` the never-seen sentinel, `$2` the offline threshold, `$3` the active
/// status, `$4` the active lease status, `$5` draining, `$6` the batch limit.
pub const SELECT_DUE_RUNNERS: &str = include_str!("select_due_runners.sql");

/// Records a runner going offline, at most once per stale episode.
///
/// The partial unique index on `(runner_id, last_seen_at)` is what makes the
/// sweep idempotent: a cycle that re-observes the same stale runner inserts
/// nothing, and the returned count tells the caller whether THIS pass was the
/// one that recorded it. Without it, a runner offline for an hour would write a
/// row every ten seconds. The metadata carries the same instant as RFC 3339,
/// written the way the heartbeat's transition event writes it.
///
/// `$1` event id, `$2` runner, `$3` event type, `$4` the metadata key, `$5` the
/// last-seen instant — which is ALSO the episode's unique stamp.
pub const INSERT_OFFLINE_EVENT: &str = include_str!("insert_offline_event.sql");

/// Releases the slots a dead runner still holds.
///
/// A fleet's slot is what stops two runners racing for the same work, so a
/// runner that died holding one would keep that fleet unrunnable until the
/// lease TTL elapsed. Each slot is stamped a moment into the past, so it reads
/// as already expired to every comparison; the matching `>` guard is what makes
/// it converge, since a slot already released is not released again.
///
/// `$1` runner, `$2` the active lease status, `$3` how far into the past.
pub const EXPIRE_ACTIVE_LEASE_SLOTS: &str = include_str!("expire_active_lease_slots.sql");

/// Finishes draining a runner once its last lease is gone, and records the
/// transition in the same statement.
///
/// The `NOT EXISTS` guard is the safety property: a draining runner still
/// holding a lease is still working, and marking it drained would tell an
/// operator it was safe to take the host away mid-run. The event is written in
/// the same statement as the status change, so history cannot disagree with
/// the row it describes.
///
/// `$1` runner, `$2` drained, `$3` draining, `$4` the active lease status, `$5`
/// event id, `$6` event type, `$7` the from-status key, `$8` the to-status key.
pub const MARK_DRAINED_IF_IDLE: &str = include_str!("mark_drained_if_idle.sql");

/// The next page of active fleets, after the cursor's position.
///
/// A KEYSET cursor, not an offset: `(updated_at, id) > ($2, $3)` resumes
/// exactly where the last page stopped, so successive passes advance through
/// the population instead of re-reading its head. The composite comparison is
/// what makes it total: `updated_at` alone repeats across fleets touched in the
/// same millisecond, and a cursor that skipped them would drop fleets rather
/// than revisit them.
///
/// `$1` the active status, `$2` the cursor's instant, `$3` the cursor's id,
/// `$4` the batch limit.
pub const SELECT_ACTIVE_FLEETS_AFTER: &str = include_str!("select_active_fleets_after.sql");

/// Fleets holding an `active` lease whose expiry has passed.
///
/// The readiness index's third writer. Its other two — ingress, and this
/// sweeper's own stream probe — both go quiet for a fleet whose stream was
/// lost while a runner held its work: nothing marks it, so nobody polls it,
/// and the lease path's reclaim, which only runs when a claim wins the fleet,
/// never gets its turn. This statement finds those fleets from the ledger and
/// the caller marks them. It flips nothing — that reclaim re-leases from
/// PostgreSQL alone and needs the lease still `active`.
///
/// `DISTINCT`, because one fleet can hold several expired leases and wants one
/// mark. Keyset on `fleet_id` after `$3`, so a fleet that stays in the set
/// until a runner claims it cannot starve the fleets behind it; the caller
/// rewinds on a short page, so the scan is cyclic.
///
/// `$1` the active status, `$2` the batch limit, `$3` the cursor's fleet id.
pub const SELECT_FLEETS_HOLDING_EXPIRED_LEASES: &str =
    include_str!("select_fleets_holding_expired_leases.sql");

/// One batch of settled leases past the retention window.
///
/// `updated_at`, not `created_at`, is the retention clock: settle and reclaim
/// both stamp it, so the window counts from the settlement the API documents.
/// `FOR UPDATE SKIP LOCKED` because every replica runs its own sweeper, and
/// with it concurrent sweepers take disjoint batches.
///
/// `$1` the terminal statuses, `$2` the retention window, `$3` the batch limit.
pub const DELETE_TERMINAL_LEASES_BATCH: &str = include_str!("delete_terminal_leases_batch.sql");

/// One batch of per-work event rows past the retention window.
///
/// Per-work tags only. The LIFECYCLE tags — enrolment, going offline, draining
/// — are never eligible, because they are the record of what a host did over
/// its whole life and an operator reading a six-month-old incident needs them.
///
/// No row lock: `api_runtime` holds no UPDATE on `runner.events`, so Postgres
/// would refuse one, and two sweeps meeting one row only means the second
/// waits and finds it gone.
///
/// `$1` the deletable event types, `$2` the retention window, `$3` the batch
/// limit.
pub const DELETE_AGED_RUNNER_EVENTS_BATCH: &str =
    include_str!("delete_aged_runner_events_batch.sql");

/// One batch of abandoned `active` leases, flipped to `expired`.
///
/// The rows nothing will ever settle: a fleet whose events stopped arriving is
/// never re-leased, so the reclaim path never reaches its stranded lease. What
/// makes an age-keyed flip safe here is the renewal CEILING — a lease anything
/// still holds is at most `MAX_RUNTIME` stale, and the retention window is
/// longer than that, so a row past it is provably abandoned.
///
/// The tally rides the same statement as the flip, so the counter cannot drift
/// from the rows it describes. A first tally for a runner writes every count,
/// since the table declares no defaults.
///
/// `$1` the active status, `$2` the retention window, `$3` expired, `$4` the
/// batch limit.
pub const EXPIRE_ABANDONED_ACTIVE_LEASES_BATCH: &str =
    include_str!("expire_abandoned_active_leases_batch.sql");

/// Every fleet, counted by its stored status.
///
/// No `WHERE` and no `LIMIT`: the answer is one row per distinct status, so
/// the result is bounded by the status vocabulary and never by the table.
pub const COUNT_FLEETS_BY_STATUS: &str = include_str!("count_fleets_by_status.sql");
