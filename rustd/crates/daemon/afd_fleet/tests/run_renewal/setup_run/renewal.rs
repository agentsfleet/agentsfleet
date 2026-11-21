//! A setup run renews as a fleet's does: for as long as its payer funds it,
//! at any age. No clock ends it.

use std::time::Duration;

use af_wire::report::RenewRequest;

use super::Case;
use crate::rows::Row;

/// How old the run is made: past the 300-second runtime setup used to have.
const PAST_FIVE_MINUTES: Duration = Duration::from_mins(6);

// Value: protects=a setup run renews past five minutes; fails_when=a setup run
// is refused or clamped at an age again; why_new=the 300-second setup runtime is
// gone (M253_003 §8); seam=the production draft assignment and renewal over live
// Postgres, the run's start aged; no_claim=the funds gate, which
// `a_setup_run_on_an_empty_wallet_is_refused` covers.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_setup_run_renews_past_five_minutes() {
    let case = Case::queued().await;
    let taken = case.take_on_host().await;
    let run = taken.run_id();
    let fixtures = &case.running.fixtures;
    fixtures
        .age(Row::Run(&run), "created_at", PAST_FIVE_MINUTES)
        .await;
    let started = fixtures
        .instant(Row::Run(&run), "created_at")
        .await
        .expect("the run row carries its start");

    let renewed = fixtures
        .plane()
        .renew_draft(
            &case.running.runner,
            &run,
            &RenewRequest::default(),
            af_core::clock::now(),
        )
        .await
        .expect("a setup run six minutes old renews");

    assert!(
        renewed > af_core::clock::after(started, PAST_FIVE_MINUTES),
        "the grant reaches past the run's age: {renewed}"
    );
    assert_eq!(
        fixtures.instant(Row::Run(&run), "run_expires_at").await,
        Some(renewed),
        "the run's row carries the grant the runner was told"
    );
    case.cleanup().await;
}
