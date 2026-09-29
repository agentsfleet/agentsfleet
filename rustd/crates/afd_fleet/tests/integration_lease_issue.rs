//! §2 against live datastores — writing the lease, and what that makes
//! reachable.
//!
//! `integration_lease_assign.rs` proves how work is FOUND. This proves what
//! happens once a runner is given it: the `fleet.runner_leases` row lands, and
//! with it the reclaim path becomes reachable at all — reclaim looks for a
//! still-`active` lease, and nothing else writes one.
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
use afd_fleet::lease::{Billed, Delivery, Kind};

use self::requests::ENROLLED_AT;
use self::seed::{MODEL, POSTURE, PROVIDER, Seeded, seeded};
use self::support::Fixtures;

/// A lapsed holder with an ISSUED lease has its work taken back, not re-pulled.
///
/// The distinction is money. A reclaim carries the dead holder's billing
/// forward and never charges again; a fresh pull would bill the same event a
/// second time. Reaching this path needs a `fleet.runner_leases` row, which is
/// why this test issues one and the sibling above does not.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_lapsed_lease_is_reclaimed_not_re_pulled() {
    let fixtures = Fixtures::create_with_queue().await;
    let Seeded {
        runners: [first, second],
        fleet,
        tenant,
        ..
    } = seeded::<2>(&fixtures).await;
    let leases = fixtures.leases();
    let now = UnixMillis::from_millis(ENROLLED_AT);

    let held = crate::seed::select_fleet_within_rotations(&leases, &first, now, &fleet)
        .await
        .expect("the fleet is leasable");
    // Hot-path write ONE, which the gate pass will own and which does not
    // exist yet. Reclaim's INNER JOIN reads this row to recover the event body
    // — without it the reclaim finds nothing and correctly falls through to a
    // fresh pull, which is what an earlier revision of this test observed.
    assert_eq!(
        leases
            .record_received(&held, now)
            .await
            .expect("the narrative log must open")
            .delivery,
        Delivery::First,
        "a newly leased event has no row yet"
    );

    let tenant_id = Uuid7::parse(&tenant).expect("the fixture id is a v7 spelling");
    let issued = leases
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
    let lease = issued.lease_id.as_str();

    assert_eq!(
        fixtures.lease_column(lease, "status").await,
        Some("active".to_owned()),
        "an issued lease opens active; that is the status reclaim looks for"
    );

    // The holder dies. Past its expiry another runner claims, and finds that
    // still-active row rather than an empty slot.
    let lapsed = held.leased_until.saturating_add_millis(1);
    let reclaimed = crate::seed::select_fleet_within_rotations(&leases, &second, lapsed, &fleet)
        .await
        .expect("a lapsed claim is winnable");

    assert_eq!(
        reclaimed.kind,
        Kind::Reclaim,
        "a lapsed holder's work is taken BACK, never pulled fresh and billed twice"
    );
    assert_eq!(
        reclaimed.event_id, held.event_id,
        "the reclaimed envelope is the one the dead holder never finished"
    );
    assert!(
        reclaimed.fence > held.fence,
        "the re-lease outranks the holder it displaced: {:?} vs {:?}",
        reclaimed.fence,
        held.fence
    );

    let carried = reclaimed
        .reused
        .as_ref()
        .expect("a reclaim carries the prior billing rather than resolving it again");
    assert_eq!(carried.tenant_id, tenant, "the wallet already charged");
    assert_eq!(carried.posture, POSTURE);
    assert_eq!(carried.model, MODEL);

    assert_eq!(
        fixtures.lease_column(lease, "status").await,
        Some("expired".to_owned()),
        "the reclaim expires the row it took the work from, in the same statement"
    );

    queue::clear_ready(fixtures.queue(), &fleet).await;
    fixtures.cleanup().await;
}

/// Tokens a slot's cursor already holds when a lease is written over it.
const METERED: i64 = 1_234;

/// A fresh lease's row resets the slot's metering cursor; a reclaim's keeps it.
///
/// The reset rides the lease insert, armed only for a fresh lease: a
/// re-leased run meters forward from where the dead holder stopped, and
/// billing it from zero would charge again for tokens already metered.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_the_lease_row_resets_the_meter_only_when_fresh() {
    let fixtures = Fixtures::create_with_queue().await;
    let Seeded {
        runners: [first, second],
        fleet,
        tenant,
        ..
    } = seeded::<2>(&fixtures).await;
    let leases = fixtures.leases();
    let now = UnixMillis::from_millis(ENROLLED_AT);
    let tenant_id = Uuid7::parse(&tenant).expect("the fixture id is a v7 spelling");
    let billed = Billed {
        tenant_id: &tenant_id,
        posture: POSTURE,
        provider: PROVIDER,
        model: MODEL,
    };

    let held = crate::seed::select_fleet_within_rotations(&leases, &first, now, &fleet)
        .await
        .expect("the fleet is leasable");
    // An earlier run's cursor, which the fresh lease must not meter from.
    fixtures.set_metered_input(&fleet, METERED).await;
    leases
        .record_received(&held, now)
        .await
        .expect("the narrative log must open");
    leases
        .issue(&first, &held, billed, now)
        .await
        .expect("the fresh lease row must be written");
    assert_eq!(
        fixtures
            .affinity_column(&fleet, crate::lease_reads::COLUMN_METERED_INPUT)
            .await,
        Some("0".to_owned()),
        "a fresh lease meters from zero"
    );

    // The holder metered some tokens, then died.
    fixtures.set_metered_input(&fleet, METERED).await;
    let lapsed = held.leased_until.saturating_add_millis(1);
    let reclaimed = crate::seed::select_fleet_within_rotations(&leases, &second, lapsed, &fleet)
        .await
        .expect("a lapsed claim is winnable");
    assert_eq!(reclaimed.kind, Kind::Reclaim);
    leases
        .issue(&second, &reclaimed, billed, lapsed)
        .await
        .expect("the reclaimed lease row must be written");
    assert_eq!(
        fixtures
            .affinity_column(&fleet, crate::lease_reads::COLUMN_METERED_INPUT)
            .await,
        Some(METERED.to_string()),
        "a reclaim meters FORWARD from the dead holder's cursor"
    );

    queue::clear_ready(fixtures.queue(), &fleet).await;
    fixtures.cleanup().await;
}
