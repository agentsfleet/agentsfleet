//! §2 against live datastores — a granted lease is a run started, and it is
//! counted as one.
//!
//! `integration_lease_assign.rs` and `integration_lease_issue.rs` prove what a
//! grant IS. This proves the grant is RECORDED: the runs-started counter moves
//! under the label the grant's kind maps to. Read back through the capturing
//! reader rather than inferred from `acquired.kind`, because the kind being
//! right says nothing about whether the producer fired.
//!
//! # Why the assertions are lower bounds
//!
//! Every lease suite in this binary grants leases, in parallel, into one
//! process-wide counter. An exact delta would be a race; what is provable here
//! is that this test's own grant moved the series it should have. The exact
//! pairing of kind to label is a unit proof beside the mapping itself.
//!
//! Marked `#[ignore]` so `make test-unit-rustd` compiles and lints these
//! without needing datastores, and `make test-integration-rustd` — which runs
//! `--ignored` and nothing else — is the only lane that executes them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use crate::queue;
use crate::requests;
use crate::seed;
use crate::support;
use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_fleet::lease::{Billed, Kind};
use afd_observability::test_util::Capture;

use self::requests::ENROLLED_AT;
use self::seed::{MODEL, POSTURE, PROVIDER, Seeded, seeded};
use self::support::Fixtures;

/// The family the grant is counted under.
const RUNS_STARTED: &str = "agentsfleet_fleet_runs_started_total";

/// Its one label.
const KIND: &str = "kind";

/// The label a fresh grant carries.
const FRESH: &str = "fresh";

/// The label a reclaim carries.
const RECLAIMED: &str = "reclaimed";

/// A fresh grant moves the `fresh` series.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_fresh_grant_counts_one_run() {
    let capture = Capture::install();
    let fixtures = Fixtures::create_with_queue().await;
    let Seeded {
        runners: [runner],
        fleet,
        ..
    } = seeded::<1>(&fixtures).await;
    let now = UnixMillis::from_millis(ENROLLED_AT);
    let before = capture.sum(RUNS_STARTED, &[(KIND, FRESH)]);

    let acquired = seed::select_fleet_within_rotations(&fixtures.leases(), &runner, now, &fleet)
        .await
        .expect("a ready fleet holding an event is leasable");
    assert_eq!(acquired.kind, Kind::Fresh);

    let after = capture.sum(RUNS_STARTED, &[(KIND, FRESH)]);
    assert!(
        after > before,
        "a fresh grant moves the fresh series: {before} -> {after}"
    );

    queue::clear_ready(fixtures.queue(), &fleet).await;
    fixtures.cleanup().await;
}

/// A reclaim over a lapsed holder moves the `reclaimed` series.
///
/// Reaching the reclaim path needs an issued lease row, exactly as
/// `test_a_lapsed_lease_is_reclaimed_not_re_pulled` sets one up.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_reclaim_counts_as_reclaimed() {
    let capture = Capture::install();
    let fixtures = Fixtures::create_with_queue().await;
    let Seeded {
        runners: [first, second],
        fleet,
        tenant,
        ..
    } = seeded::<2>(&fixtures).await;
    let leases = fixtures.leases();
    let now = UnixMillis::from_millis(ENROLLED_AT);

    let held = seed::select_fleet_within_rotations(&leases, &first, now, &fleet)
        .await
        .expect("the fleet is leasable");
    leases
        .record_received(&held, now)
        .await
        .expect("the narrative log must open");
    let tenant_id = Uuid7::parse(&tenant).expect("the fixture id is a v7 spelling");
    leases
        .issue(
            &first,
            &held,
            Billed {
                tenant_id: &tenant_id,
                posture: POSTURE,
                provider: PROVIDER,
                model: MODEL,
            },
            now,
        )
        .await
        .expect("the lease row must be written");

    let before = capture.sum(RUNS_STARTED, &[(KIND, RECLAIMED)]);
    let lapsed = held.leased_until.saturating_add_millis(1);
    let reclaimed = seed::select_fleet_within_rotations(&leases, &second, lapsed, &fleet)
        .await
        .expect("a lapsed claim is winnable");
    assert_eq!(reclaimed.kind, Kind::Reclaim);

    let after = capture.sum(RUNS_STARTED, &[(KIND, RECLAIMED)]);
    assert!(
        after > before,
        "a reclaim moves the reclaimed series: {before} -> {after}"
    );

    queue::clear_ready(fixtures.queue(), &fleet).await;
    fixtures.cleanup().await;
}
