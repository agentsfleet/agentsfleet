//! The run bookkeeping both kinds share: claim, issue, renew, meter, settle
//! and reclaim on `runner.slots` and `runner.runs`, keyed by a slot's or a
//! run's id and blind to whose work it is.
//!
//! A slot's id is its owner's ([`crate::run::RunOwner`]), and a run
//! reaches its slot through `COALESCE(fleet_id, draft_id)`, so nothing here
//! reads a fleet or authoring table. A statement that names an owner takes it
//! as two parameters, bound from [`crate::run::RunOwner::fleet_id`] and
//! [`crate::run::RunOwner::draft_id`]; one keyed by a slot takes
//! [`crate::run::RunOwner::slot_id`]. What an owner adds, a fleet's event
//! and admission or a draft's message, is that kind's own statement in the
//! same transaction.
//!
//! # Where the fleet keeps a fused statement instead
//!
//! The fleet's claim, issue, renewals and report each run ONE statement that
//! does this bookkeeping beside the fleet's own rows
//! (`../run/claim_admission_head.sql`, `../run/insert_run_with_event.sql`,
//! `../renew_batch.sql`, `../meter_run.sql`, `../commit_report.sql`).
//! Splitting them into a shared statement and a fleet one would add a round
//! trip to every fleet chat, so they stay fused. [`INSERT_RUN`],
//! [`EXTEND_RUN`], [`SETTLE_RUN`] and [`EXPIRE_PRIOR_RUN`] are the same
//! bookkeeping without the fleet's rows: the draft kind binds them, with its
//! own per-kind statements beside them. A change to the fencing, the clock or
//! the metering in one belongs in its fused fleet twin in the same diff.
//!
//! # The setup turns the authoring path still runs
//!
//! [`INSERT_RUN`]'s census counts `authoring.attempts` beside the runs,
//! as every host census does while the authoring path still issues attempts.
//! That term goes with the table.

/// Claims a slot for its owner, bumping the monotonic fence.
///
/// `$1` the owning fleet, NULL for a draft, `$2` the owning draft, NULL for a
/// fleet, `$3` runner, `$4` the run lifetime, `$5` how long a silent runner
/// stays live, `$6` the runner status that may run. The owner's one non-NULL
/// column is the slot's key, so a bind naming both or neither fails.
///
/// Answers no row when a live runner still holds the slot: that absence is
/// the taken verdict, not an error. While `hold_expires_at` is in the future
/// and its holder can still run, only the holder wins, and the holder's own
/// claim keeps the hold. `prior` reads the slot before the statement changes
/// it, so the caller can tell a claim on a held slot from one on an unheld
/// slot. One `clock_timestamp()` reading decides every deadline.
///
/// The fleet's production claim runs this upsert as the `slot` step of
/// `claim_admission_head.sql`; the test seam `Runs::claim` runs it alone.
pub const CLAIM_SLOT: &str = include_str!("claim_slot.sql");

/// Releases a slot, fencing-guarded so only the current holder can free it,
/// and records whether that holder keeps the owner's sandbox.
///
/// `$1` slot, `$2` fence, `$3` when the runner asked its hold to lapse (NULL
/// for none), `$4` the longest hold. The hold written is the ask capped at one
/// `$4` window from `clock_timestamp()`, and none when the ask is already past.
/// The freed deadline is written one millisecond behind the clock: a
/// `TIMESTAMPTZ(3)` column rounds, and a deadline rounded up past the instant
/// a claim reads a moment later would leave a released slot looking held.
///
/// The guard is load-bearing: a holder superseded by a reclaim would otherwise
/// free the current holder's slot and hand one owner's work to two runners.
/// Idempotent: a no-op once the token has moved on.
pub const RELEASE_SLOT: &str = include_str!("release_slot.sql");

/// [`RELEASE_SLOT`] for a claim that held nothing. `$1` slot, `$2` fence.
///
/// It drops the sticky hint too, or an owner whose pass keeps stopping sorts
/// first for that runner on every poll, starving the rest. A slot still held
/// keeps its hint: the hint names the holder, and the hold steers the owner's
/// next work there.
pub const RELEASE_UNLEASED_SLOT: &str = include_str!("release_unleased_slot.sql");

/// Locks a run's slot and then the run, and answers whether its Kill mark
/// is set. `$1` run, `$2` runner.
///
/// Slot then run is every renewal's lock order, held until the renewal
/// commits or is refused; a run that is not this runner's answers no row.
pub const LOCK_CANCELLED: &str = include_str!("lock_cancelled.sql");

/// The hold a settled run's report recorded, while it is still live.
///
/// `$1` run, `$2` runner, `$3` the reported status. A receipt read never
/// renews or recreates a hold after a later issue consumed it.
pub const REPORT_HOLD: &str = include_str!("report_hold.sql");

/// Issues a run under its claim's fence, with the host's placement choice,
/// an audit row and the runner's tally, in one statement.
///
/// `$1` run, `$2` runner, `$3` the owning fleet, NULL for a draft, `$4` the
/// owning draft, NULL for a fleet, `$5` workspace, `$6` tenant, `$7` the event
/// or draft message, `$8` its producer's instant, `$9` posture, `$10`
/// provider, `$11` model, `$12` the claim's fence, `$13` active status, `$14`
/// audit row, `$15` its event type, `$16` its metadata, `$17` decision
/// snapshot (NULL for none), `$18` the deadline the claim answered, `$19` the
/// host's run version as read, `$20` the run type chosen
/// ([`crate::run::RunOwner::kind`]), `$21` the most seconds it may run.
///
/// The caller holds the host row and has proved the owner's scope in the same
/// transaction. No row when the host is full, the fence moved or the choice
/// moved; the caller rolls back.
pub const INSERT_RUN: &str = include_str!("insert_run.sql");

/// Extends one live run and its slot to one clamped deadline and measures
/// the run time since its cursor.
///
/// `$1` run, `$2` runner, `$3` milliseconds per second, `$4` active status,
/// `$5` the run lifetime, `$6` the most a run may run. Answers
/// `probe_found`, `new_until` (NULL when nothing moved) and `hard_cap`, and the
/// slice the caller's usage line charges: `checked_at`, `started_at`, `d_ms`,
/// `runtime_seq`, `tenant_id`, `workspace_id` and `event_id`. Run after
/// [`LOCK_CANCELLED`], under the host row.
pub const EXTEND_RUN: &str = include_str!("extend_run.sql");

/// Settles one run at its report: status, cursor, slot release with any
/// hold, tally and audit row, fenced on the run's own token.
///
/// `$1` run, `$2` runner, `$3` milliseconds per second, `$4` active status,
/// `$5` reported status, `$6` succeeded, `$7` the hold asked for (NULL for
/// none), `$8` the hold's idle cap, `$9` audit row, `$10` its event type,
/// `$11` its metadata, `$12` the longest runtime one line may claim,
/// `$13`-`$14` the opening's credential proof and connection (NULL for a
/// trusted call), `$15` the host states that may keep a hold. Answers
/// `claimed`, `accepted_hold`, the run's latest `run_status`, `fence_holds`
/// and the final slice. The caller has locked the owner's scope first.
pub const SETTLE_RUN: &str = include_str!("settle_run.sql");

/// Expires the newest earlier run of the work a claim just took, when its
/// holder lapsed without reporting, and tallies that expiry.
///
/// `$1` the owning fleet, NULL for a draft, `$2` the owning draft, NULL for a
/// fleet, `$3` the event or draft message, `$4` active status, `$5` expired
/// status. Answers the predecessor's id, if any, and how many runs the work
/// had before this claim. Run under the slot the claim won.
pub const EXPIRE_PRIOR_RUN: &str = include_str!("expire_prior_run.sql");
