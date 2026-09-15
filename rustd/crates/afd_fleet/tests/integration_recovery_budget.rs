//! Dimension 7.1 — the readiness index gains a Postgres-driven writer, and it
//! pays for itself without spending anything on the idle path.
//!
//! # The gap this closes
//!
//! Two writers marked a fleet ready before this: ingress, as it admits, and
//! the reclaim sweeper, when the STREAM says the fleet still holds work. A
//! fleet whose lease expired while its stream was lost is marked by neither,
//! so no runner polls it, and `reclaim_prior_active` never fires — it only
//! runs once a claim has won that fleet. The sweeper therefore asks the ledger
//! too: which fleets hold an `active` lease past `lease_expires_at`?
//!
//! # What the answer may and may not do
//!
//! A readiness mark ONLY. The sweeper never flips the lease, because
//! `reclaim_prior_active` re-leases from Postgres alone, before the stream is
//! read, and requires the lease still `active` — a flipped lease is invisible
//! to the one path that recovers it. That is asserted between the pass and the
//! poll, since the claim path expires the dead lease itself and an assertion
//! taken afterwards cannot tell the two writers apart.
//!
//! # And the idle path stays free
//!
//! Discovery reads Postgres in the SWEEPER and never on the empty-poll path,
//! whose zero-Postgres property is what makes idle cost scale with runner
//! count instead of runners times fleets. Proven by reading back the tally the
//! poll publishes for the operator gauges: a query that was never issued
//! leaves nothing else behind to assert on.
//!
//! # Not asserted here, named rather than implied
//!
//! The wrapping cursor at one past the sweeper's page bound: staged at
//! `BATCH_LIMIT + 1` fleets by `test_cluster_restart_and_stale_snapshot_preserve_obligations`,
//! which needs that population for its own reasons and proves the wrap with
//! it. Foreground p95 and per-statement `EXPLAIN` wait for traffic that can be
//! measured, and the spec records them as owed.
//!
//! The database is private to this test: the replay budget counts unconfirmed
//! rows deployment-WIDE, and on the shared lane a sibling suite's deferred row
//! would keep the count up and the refusal would never be seen to clear.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use std::time::Duration;

use afd_admission::{Admissions, Budgets};
use afd_core::clock;
use afd_datastore::FleetStreams;
use afd_datastore::ready::READY_PARTITIONS;
use afd_fleet::lease::{Leases, runner_consumer};
use afd_runner::sweep::Sweep as _;
use afd_runner::sweep::rebuild::rebuild;
use afd_runner::sweep::reclaim::Reclaim;
use afd_runner::sweep::reconcile::Reconcile;
use afd_runner::sweep::replay::Replay;

use crate::queue;
use crate::recovery_seed::{abandoned_mid_flight, admission, ledger};
use crate::seed::{seeded_parts, select_within_one_rotation};
use crate::support::Fixtures;

/// The budget the dimension states: a fleet the ledger surfaced is obtained
/// by the ordinary claim path inside this many reclaim passes.
const RECLAIM_PASSES: u32 = 2;

/// `fleet.runner_leases.status` while a holder still owns the work.
const LEASE_ACTIVE: &str = "active";

/// The same column once the claim path has taken the work off a dead holder.
const LEASE_EXPIRED: &str = "expired";

/// Rows left waiting for a receipt, and the ceiling set below them.
///
/// Small because the property is the refusal's shape, not the production
/// number — `afd_admission::budget` asserts that one at compile time.
const DEFERRED_ROWS: u64 = 4;

/// One round covers the rows above: the replay pass takes a larger batch.
const REBUILD_ROUNDS: u32 = 1;

/// How long a refusal holds its figure before resampling, plus margin.
///
/// `afd_admission::budget::ceiling` resamples a REFUSING estimate every 500 ms
/// — shorter than the healthy interval precisely so a refusal can clear as the
/// backlog drains. Waiting it out is what proves the clear happens on the
/// ledger that refused, rather than on a fresh one with an empty counter.
const REFUSAL_RESAMPLE_WAIT: Duration = Duration::from_millis(750);

/// The lease's status column, as stored.
async fn lease_status(fixtures: &Fixtures, lease: &str) -> Option<String> {
    fixtures.lease_column(lease, "status").await
}

/// The ledger's answer reaches the ordinary claim path, and costs the lease
/// nothing on the way.
async fn recovers_an_abandoned_lease(fixtures: &Fixtures, leases: &Leases) {
    let staged = abandoned_mid_flight(fixtures, leases).await;
    let reclaim = Reclaim::new(
        fixtures.database.clone(),
        fixtures.queue().clone(),
        runner_consumer(),
    );

    let mut obtained = None;
    for _pass in 0..RECLAIM_PASSES {
        let swept = reclaim
            .sweep()
            .await
            .expect("a reclaim pass runs against both live datastores");
        assert!(
            swept.scanned > 0,
            "the pass walked the active fleets it was meant to: {swept:?}"
        );
        assert_eq!(
            lease_status(fixtures, &staged.lease_id).await.as_deref(),
            Some(LEASE_ACTIVE),
            "the sweeper marks and never flips: a flipped lease is invisible to reclaim_prior_active"
        );
        obtained = select_within_one_rotation(leases, &staged.poller, clock::now()).await;
        if obtained.is_some() {
            break;
        }
    }

    let acquired = obtained.expect(
        "a fleet holding an expired lease is marked by the ledger question and offered to the next poll",
    );
    assert_eq!(
        acquired.event_id, staged.event_id,
        "the poll re-leases the event the dead holder never finished, not a fresh one"
    );
    assert_eq!(
        lease_status(fixtures, &staged.lease_id).await.as_deref(),
        Some(LEASE_EXPIRED),
        "the CLAIM path expired the dead lease, on its way to re-leasing the work"
    );
    assert_eq!(
        fixtures.admissions_for(&staged.fleet).await,
        1,
        "recovery re-leases identities; it never mints them"
    );

    // The sweeper re-marked this fleet on its way. Leaving the mark behind
    // would crowd the bounded peek every other suite in this binary depends
    // on — see `queue::clear_ready`.
    queue::clear_ready(fixtures.queue(), &staged.fleet).await;
}

/// The empty poll answers without asking Postgres anything.
///
/// Read off the tally the poll itself publishes, through the `test-util` seam
/// beside `select`, because a query that was never issued leaves no row, no
/// error and no lease to assert on. The peek is bounded and the index is
/// shared, so a rotation is walked and every poll that scanned nothing is
/// checked; the count guards against a run where a sibling suite kept every
/// partition busy and the empty path was never entered at all.
async fn an_empty_poll_reaches_no_database(fixtures: &Fixtures) {
    let leases = fixtures.leases();
    let (fleet, _workspace, _tenant, [runner]) = seeded_parts::<1>(fixtures).await;
    queue::clear_ready(fixtures.queue(), &fleet).await;

    let mut empty_polls = 0_u16;
    for _poll in 0..READY_PARTITIONS {
        let (outcome, cost) = leases.select_measured(&runner, clock::now()).await;
        if cost.candidates_scanned > 0 {
            continue;
        }
        empty_polls += 1;
        assert_eq!(
            cost.database_roundtrips, 0,
            "an empty peek returns before the candidate query, so nothing reaches Postgres"
        );
        assert!(
            matches!(outcome, Ok(None)),
            "and it answers no-work rather than faulting"
        );
    }
    assert!(
        empty_polls > 0,
        "every one of the {READY_PARTITIONS} readiness partitions held a foreign mark, so the empty path was never entered"
    );
}

/// Rows the queue never confirmed: committed, `receipt IS NULL`.
///
/// The state the replay budget counts, and the state recovery returns rows
/// to — so it is both what raises the backlog here and what the rebuild
/// below drains.
async fn defer_rows_awaiting_receipt(fixtures: &Fixtures, fleet: &str, workspace: &str) {
    let deferring = Admissions::for_tests(fixtures.database.clone(), queue::unreachable());
    for index in 0..DEFERRED_ROWS {
        let key = format!("deferred-{index}");
        deferring
            .admit(admission(fleet, workspace, &key))
            .await
            .expect("the row commits whatever the queue does");
    }
}

/// The operator's own repopulate, which is what returns the receipts.
async fn repopulate(fixtures: &Fixtures) {
    let admissions = ledger(fixtures);
    let reconcile = Reconcile::new(admissions.clone());
    let replay = Replay::for_rebuild(admissions);
    rebuild(&[&reconcile, &replay], REBUILD_ROUNDS)
        .await
        .expect("every recovery pass runs against both live datastores");
}

/// The deployment budget refuses while recovery's rows wait for a receipt,
/// and admits again once the rebuild has given them one.
async fn the_replay_budget_refuses_and_clears(fixtures: &Fixtures) {
    let (fleet, workspace, _tenant, [_runner]) = seeded_parts::<1>(fixtures).await;
    FleetStreams::new(fixtures.queue().clone())
        .ensure_group(&fleet)
        .await
        .expect("the fleet's consumer group exists before the replay appends to it");

    defer_rows_awaiting_receipt(fixtures, &fleet, &workspace).await;

    let ceiling = DEFERRED_ROWS - 1;
    let budgeted = ledger(fixtures).with_budgets(Budgets {
        fleet_backlog: u64::MAX,
        replay_backlog: ceiling,
    });
    let before = fixtures.admissions_for(&fleet).await;
    let refused = budgeted
        .admit(admission(&fleet, &workspace, "over-ceiling"))
        .await
        .expect_err("a ceiling below the waiting rows refuses new work");
    assert!(
        refused.is_over_capacity(),
        "the refusal carries the capacity class an operator reads: {refused}"
    );
    assert_eq!(
        fixtures.admissions_for(&fleet).await,
        before,
        "a refusal commits nothing: the row is what the budget protects"
    );

    repopulate(fixtures).await;

    tokio::time::sleep(REFUSAL_RESAMPLE_WAIT).await;
    budgeted
        .admit(admission(&fleet, &workspace, "after-recovery"))
        .await
        .expect("the refusal clears once the backlog it counted has its receipts");

    FleetStreams::new(fixtures.queue().clone())
        .forget(&fleet)
        .await
        .expect("cleanup");
    queue::clear_ready(fixtures.queue(), &fleet).await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_recovery_finds_every_unfinished_fleet_within_budget() {
    let fixtures = Fixtures::create_isolated_with_queue().await;
    let leases = fixtures.leases();

    recovers_an_abandoned_lease(&fixtures, &leases).await;
    an_empty_poll_reaches_no_database(&fixtures).await;
    the_replay_budget_refuses_and_clears(&fixtures).await;

    fixtures.cleanup().await;
}
