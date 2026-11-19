//! §2 against live datastores — writing the run, and what that makes
//! reachable.
//!
//! `integration_run_assign.rs` proves how work is FOUND. This proves what
//! happens once a runner is given it: the `thread.runs` row lands, and
//! with it the reclaim path becomes reachable at all — reclaim looks for a
//! still-`active` run, and nothing else writes one.
//!
//! Marked `#[ignore]` so `make test-unit-rustd` compiles and lints these
//! without needing datastores, and `make test-integration-rustd` — which runs
//! `--ignored` and nothing else — is the only lane that executes them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use crate::seed;
use crate::support;
use afd_fleet::run::{Billed, Delivery, Kind};

use self::seed::{MODEL, POSTURE, PROVIDER, Seeded, seeded};
use self::support::Fixtures;
use crate::rows::{LAPSE, Row};

/// A lapsed holder with an ISSUED run has its work taken back, not re-pulled.
///
/// The distinction is money. A reclaim carries the dead holder's billing
/// forward and never charges again; a fresh pull would bill the same event a
/// second time. Reaching this path needs a `thread.runs` row, which is
/// why this test issues one and the sibling above does not.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_lapsed_run_is_reclaimed_not_re_pulled() {
    let fixtures = Fixtures::create().await;
    let Seeded {
        runners: [first, second],
        fleet,
        tenant,
        ..
    } = seeded::<2>(&fixtures).await;
    let runs = fixtures.runs();
    let now = fixtures.enrolled_at();

    let held = seed::select_fleet(&runs, &first, &fleet)
        .await
        .expect("the fleet is leasable");
    // Hot-path write ONE, which the gate pass will own and which does not
    // exist yet. Reclaim's INNER JOIN reads this row to recover the event body
    // — without it the reclaim finds nothing and correctly falls through to a
    // fresh pull, which is what an earlier revision of this test observed.
    assert_eq!(
        held.opened.received.delivery,
        Delivery::First,
        "a newly held event has no row yet"
    );

    let tenant_id = tenant.clone();
    let issued = runs
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
        .expect("the run row must be written")
        .expect("the claim is still held");
    let run = &issued.run_id;

    assert_eq!(
        fixtures.text(Row::Run(run), "status").await,
        Some("active".to_owned()),
        "an issued run opens active; that is the status reclaim looks for"
    );

    // The holder dies. Past its expiry another runner claims, and finds that
    // still-active row rather than an empty slot.
    fixtures
        .age(Row::Slot(&fleet), "run_expires_at", LAPSE)
        .await;
    let reclaimed = seed::select_fleet(&runs, &second, &fleet)
        .await
        .expect("a lapsed claim is winnable");

    assert_eq!(
        reclaimed.kind,
        Kind::Reclaim,
        "a lapsed holder's work is taken BACK, never pulled fresh and billed twice"
    );
    assert_eq!(
        reclaimed.turn_id, held.turn_id,
        "the reclaimed envelope is the one the dead holder never finished"
    );
    assert!(
        reclaimed.fence > held.fence,
        "the re-run outranks the holder it displaced: {:?} vs {:?}",
        reclaimed.fence,
        held.fence
    );

    assert_eq!(
        fixtures.text(Row::Run(run), "status").await,
        Some("expired".to_owned()),
        "the reclaim expires the row it took the work from, in the same statement"
    );

    fixtures.cleanup().await;
}

/// Every run meters its own run, from the instant it was issued.
///
/// A fresh run and a reclaim each open their own runtime cursor at issue:
/// the reclaimed run bills from its own start, and the time between the dead
/// holder's deadline and the reclaim is billed to nobody, because nothing ran.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_every_run_meters_from_its_own_issue() {
    let fixtures = Fixtures::create().await;
    let Seeded {
        runners: [first, second],
        fleet,
        tenant,
        ..
    } = seeded::<2>(&fixtures).await;
    let runs = fixtures.runs();
    let tenant_id = tenant.clone();
    let billed = Billed {
        tenant_id: &tenant_id,
        posture: POSTURE,
        provider: PROVIDER,
        model: MODEL,
    };

    let held = seed::select_fleet(&runs, &first, &fleet)
        .await
        .expect("the fleet is leasable");
    runs.issue(&first, &held, billed, fixtures.enrolled_at())
        .await
        .expect("the fresh run row must be written")
        .expect("the claim is still held");
    let dead_holders_cursor = fixtures.metered_since(&fleet).await;

    fixtures
        .age(Row::Slot(&fleet), "run_expires_at", LAPSE)
        .await;
    let reclaimed = seed::select_fleet(&runs, &second, &fleet)
        .await
        .expect("a lapsed claim is winnable");
    assert_eq!(reclaimed.kind, Kind::Reclaim);
    runs.issue(&second, &reclaimed, billed, fixtures.enrolled_at())
        .await
        .expect("the reclaimed run row must be written")
        .expect("the claim is still held");
    assert!(
        fixtures.metered_since(&fleet).await > dead_holders_cursor,
        "a reclaim meters from its own issue, never the dead holder's cursor"
    );

    fixtures.cleanup().await;
}

/// A run insert under a claim this pass no longer holds writes nothing.
///
/// The pass that outlived its claim lost the slot to a runner that took the
/// same entry over. Its insert must write no run, or two runners execute one
/// event. Both arms of the fence are driven: a claim that merely lapsed, then
/// one a second runner won.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_stale_claim_writes_no_run() {
    let fixtures = Fixtures::create().await;
    let Seeded {
        runners: [first, second],
        fleet,
        tenant,
        ..
    } = seeded::<2>(&fixtures).await;
    let runs = fixtures.runs();
    let now = fixtures.enrolled_at();
    let tenant_id = tenant.clone();
    let billed = Billed {
        tenant_id: &tenant_id,
        posture: POSTURE,
        provider: PROVIDER,
        model: MODEL,
    };
    let held = seed::select_fleet(&runs, &first, &fleet)
        .await
        .expect("the fleet is leasable");
    // The slot's deadline passes by the database clock, which the fenced
    // insert checks; the caller's clock decides nothing there.
    fixtures
        .age(Row::Slot(&fleet), "run_expires_at", LAPSE)
        .await;

    let expired = runs
        .issue(&first, &held, billed, fixtures.enrolled_at())
        .await
        .expect("the fenced insert answers");
    assert!(expired.is_none(), "a lapsed claim writes no run");

    let won = runs
        .claim(&held.fleet_id, &second, af_core::timing::RUN_TTL)
        .await
        .expect("the claim statement answers");
    assert!(
        won.is_some(),
        "the lapsed slot is winnable by another runner"
    );
    let superseded = runs
        .issue(&first, &held, billed, now)
        .await
        .expect("the fenced insert answers");
    assert!(superseded.is_none(), "a superseded fence writes no run");
    let mut connection = fixtures.database.acquire().await.expect("database");
    let active: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM thread.runs WHERE fleet_id = $1 AND status = 'active'",
    )
    .bind(&fleet)
    .fetch_one(&mut *connection)
    .await
    .expect("active runs");
    assert_eq!(active, 0, "a lost claim wrote no active run");
    drop(connection);

    fixtures.cleanup().await;
}

// Value: protects=a plain chat whose fleet's setup moved after the claim gets
// no run, now that the insert carries the setup proof; fails_when=the insert
// drops the head guard and a run runs the old setup; why_new=the check moved
// out of `issue_source.sql` for plain admissions; seam=none;
// no_claim=a durable task, which still rereads its source.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_plain_run_under_a_moved_setup_is_not_written() {
    let fixtures = Fixtures::create().await;
    let Seeded {
        runners: [first, _second],
        fleet,
        tenant,
        ..
    } = seeded::<2>(&fixtures).await;
    let runs = fixtures.runs();
    let billed = Billed {
        tenant_id: &tenant,
        posture: POSTURE,
        provider: PROVIDER,
        model: MODEL,
    };
    let held = seed::select_fleet(&runs, &first, &fleet)
        .await
        .expect("the fleet is leasable");
    let claimed: af_core::id::Uuid7 =
        sqlx::query_scalar("SELECT head_revision_id FROM fleet.fleets WHERE id = $1")
            .bind(&fleet)
            .fetch_one(&mut *fixtures.database.acquire().await.expect("observer"))
            .await
            .expect("the seeded fleet has a head");
    let proof = afd_fleet::run::installed::IssueRevision {
        accepted: claimed.clone(),
        current: claimed,
        task_generation: None,
    };
    fixtures
        .native_config(&fleet, r#"{"name":"moved-setup","x-agentsfleet":{"triggers":[],"tools":[],"budget":{"daily_nanos":"1000000000000"}}}"#)
        .await;

    let issued = runs
        .issue_for_revision(&first, &held, billed, fixtures.enrolled_at(), Some(&proof))
        .await
        .expect("the issue answers");

    assert!(issued.is_none(), "a moved head writes no run");
    let written: i64 = sqlx::query_scalar("SELECT count(*) FROM thread.runs WHERE fleet_id = $1")
        .bind(&fleet)
        .fetch_one(&mut *fixtures.database.acquire().await.expect("observer"))
        .await
        .expect("run count");
    assert_eq!(written, 0);
    fixtures.cleanup().await;
}
