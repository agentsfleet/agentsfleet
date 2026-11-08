//! The terminal report's reads. The settle itself is
//! [`super::meter::METER_LEASE`] with [`super::meter::Ending::Settle`]: one
//! statement that claims the lease and settles the money.
//!
//! # Why the claim and the settle are ONE statement
//!
//! They authorize each other. The fence that says this runner may report is the
//! same fence that says its final slice may be charged, and a reclaim that
//! bumps `fencing_token` between the two would leave one of them done and the
//! other refused. Split in two, the cap path loses the last slice — the reclaim
//! wins the fence while the report is still pricing — and no amount of retrying
//! recovers it, because by then the lease is somebody else's.
//!
//! Fused, a racing reclaim blocks on the slot row lock until this commits.
//! By the time it runs, the lease is `reported` and the slice is charged.

/// The lease a report is about, scoped to the runner presenting it.
///
/// The `runner_id` predicate IS the ownership check, and it is why a foreign
/// lease id and an unknown one answer the same 404: this statement cannot see
/// another runner's row, so the handler has nothing to leak. An id-only lookup
/// with an ownership comparison afterwards would put the two facts in different
/// places and make the endpoint an oracle for which lease ids are live.
///
/// `status` is deliberately ABSENT from the predicate. The claim below applies
/// it, and reading it here as well would let a lease flip between the two
/// statements and answer 404 for a row the claim would have fenced — two
/// different refusals for one race, decided by timing.
///
/// `$1` lease, `$2` runner.
pub const SELECT_LEASE_FOR_REPORT: &str = include_str!("select_lease_for_report.sql");

/// What a claim that matched no row MEANT, for the lease it named.
///
/// [`super::meter::METER_LEASE`] settles only an `active` lease, so it answers the same
/// empty claim to two different situations: a holder the fleet superseded
/// while it ran, and this same runner re-sending a report it already settled
/// because the response was lost. The first is owed a refusal. The second is
/// owed the outcome it already has — a finished run whose answer is retried
/// into a permanent refusal is an answer thrown away, which is the failure
/// RULE IDMP names.
///
/// Scoped by `runner_id` for the reason [`SELECT_LEASE_FOR_REPORT`] gives: the
/// statement cannot see another runner's row, so it can leak nothing. Read
/// INSIDE the settle's transaction, on the snapshot the claim that missed
/// decided against — read outside it, a reclaim landing between the two would
/// answer about a lease the claim never saw.
///
/// The fence is deliberately absent. A settled report releases the fleet's
/// slot, and the next event's claim bumps `fencing_token` past this runner's
/// token within milliseconds — so a fence comparison here would call a
/// correctly settled report superseded, which is precisely the answer this
/// statement exists to stop. The lease's own status is the fact that decides:
/// `reported` is this runner's row, already terminal.
///
/// `$1` lease, `$2` runner.
pub const SELECT_LEASE_DISPOSITION: &str = include_str!("select_lease_disposition.sql");

/// Record that a lease was given back.
///
/// Run by `Leases::record_released` in `finalize.rs`. The closing bracket of the
/// `lease_acquired` row [`super::lease::INSERT_LEASE_WITH_EVENT`] writes, and
/// deliberately a separate statement rather than a CTE on the settle: it is
/// best-effort audit, and a datastore blip writing history must not fail a
/// report whose money has already committed.
///
/// `$1` row id, `$2` runner, `$3` event type, `$4`–`$9` the three
/// metadata key/value pairs.
pub const INSERT_RUNNER_EVENT: &str = include_str!("insert_runner_event.sql");
