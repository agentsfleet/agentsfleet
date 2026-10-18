//! The stage half of the ledger key: renewals accumulate into one row per fleet.
//!
//! Split from [`super`] by the charge type it drives. The receive charges there
//! prove the key tells two fleets apart; this proves the narrower key still
//! FINDS the row the last renewal left, so the accumulate arm keeps firing.

use sqlx::Row as _;

use crate::queue;
use crate::report_seed::{Held, held};

/// A budget deep enough that forty renewals cannot exhaust it.
///
/// The ceiling is not what this test is about, and a run that halted
/// mid-way would assert a smaller sum and still pass.
const DEEP_DAILY_BUDGET: &str = r#"{"name":"ledger-scope","x-agentsfleet":{"triggers":[],"tools":[],"budget":{"daily_nanos":"1000000000000"}}}"#;

/// How many renewals one run makes here.
///
/// A live run renews every ~25 seconds, so forty is an unremarkable hour. The
/// number matters because the invariant is about REPETITION: one renewal
/// proves the insert, and only many prove the conflict arm never multiplies.
const RENEWALS: usize = 40;

/// Raises the fleet's ceiling so the renewals below are not budget-gated.
async fn fund(held: &Held) {
    held.fixtures
        .native_config(&held.fleet, DEEP_DAILY_BUDGET)
        .await;
}

/// Meters one renewal through the production plane.
async fn renew(held: &Held) {
    let plane = held.fixtures.plane();
    plane
        .renew(
            &held.runner,
            held.issued.lease_id.as_str(),
            afd_wire::report::RenewRequest::default(),
            held.now,
        )
        .await
        .expect("a funded lease renews");
    drop(plane);
}

/// The stage rows this fleet's event holds, and what they total.
async fn stage_rows(held: &Held) -> (i64, i64) {
    let mut connection = held
        .fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    let row = sqlx::query(
        "SELECT count(*), COALESCE(SUM(charged_nanos), 0)::bigint
         FROM billing.fleet_charges
         WHERE fleet_id = $1::uuid AND event_id = $2 AND charge_type = 'stage'",
    )
    .bind(&held.fleet)
    .bind(&held.event_id)
    .fetch_one(&mut *connection)
    .await
    .expect("reading the stage rows");
    (
        row.try_get::<i64, _>(0).expect("count decodes"),
        row.try_get::<i64, _>(1).expect("sum decodes"),
    )
}

/// Dimension 2.4. Repeated renewals accumulate into one stage row per fleet.
///
/// The regression the narrowed arbiter could most easily have caused. The
/// accumulate arm's whole job is that a renewal finds the row the last one
/// left and adds to it; an arbiter that stopped matching would insert a fresh
/// row per tick instead, and the table would grow without bound on every live
/// run while the totals still looked plausible row by row.
///
/// Forty rather than two, because two proves the conflict arm fires once and
/// says nothing about it continuing to fire.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn renewal_accumulates_per_fleet_event() {
    let held = held().await;
    fund(&held).await;

    for _ in 0..RENEWALS {
        renew(&held).await;
    }

    let (rows, total) = stage_rows(&held).await;
    assert_eq!(
        rows, 1,
        "forty renewals must accumulate into ONE stage row — a row per tick is \
         a table that grows with a run's length"
    );
    // Not asserted to have MOVED: the metered amount is the renewal request's
    // own token deltas, and `RenewRequest::default()` carries none. A test
    // that demanded a non-zero total would be asserting the fixture's payload
    // rather than the conflict arm this file is about.
    assert!(
        total >= 0,
        "the accumulated row must decode as a charge, whatever it totals"
    );

    queue::clear_ready(held.fixtures.queue(), &held.fleet).await;
    held.fixtures.cleanup().await;
}
