//! The refusals of a host's renewal frame: each refused lease is answered on
//! its own, and the log says why. Companion to `integration_renew_batch.rs`.
//!
//! Marked `#[ignore]`; `make test-integration-rustd` runs them.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use af_core::error_code::{
    RUN_BUDGET_EXCEEDED, RUN_LEASE_CANCELLED, RUN_LEASE_NOT_FOUND, RUN_LEASE_RENEWAL_NO_CREDITS,
};
use af_core::test_util::trace::Capture;

use crate::integration_renew_batch::{SEEDED_DAILY_NANOS, codes, frame, spend};
use crate::report_seed::held;
use crate::rows::Row;

// Value: protects=a lease another runner holds is refused alone;
// fails_when=one foreign id refuses the frame; why_new=the frame is one
// statement now; seam=none; no_claim=a malformed id, which the gateway refuses
// before the batch.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn host_batch_answers_foreign_lease_lost_alone() {
    let held = held().await;
    let sibling = held.hold_another().await;
    let foreign = crate::report_seed::held().await;

    let answers = frame(
        &held,
        &[
            &held.issued.lease_id,
            &foreign.issued.lease_id,
            &sibling.lease,
        ],
        false,
    )
    .await;

    assert_eq!(
        codes(&answers),
        [None, Some(RUN_LEASE_NOT_FOUND), None],
        "{answers:?}"
    );
    foreign.fixtures.cleanup().await;
    held.fixtures.cleanup().await;
}

// Value: protects=a fleet at its ceiling stops alone, and one nano under runs
// on; fails_when=the statement compares as `>` rather than `budget::covers`'s
// reached, or one fleet's ceiling stops another; why_new=the ceiling is read
// in the statement; seam=none; no_claim=a monthly ceiling, which the seeded
// setup does not declare.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn host_batch_refuses_over_budget_fleet_alone() {
    let held = held().await;
    let under = held.hold_another().await;
    let workspace = held.acquired.workspace_id.clone();
    spend(
        &held,
        &held.tenant,
        &workspace,
        &held.fleet,
        SEEDED_DAILY_NANOS,
    )
    .await;
    spend(
        &held,
        &under.tenant,
        &under.workspace,
        &under.fleet,
        SEEDED_DAILY_NANOS - 1,
    )
    .await;

    let log = Capture::install();
    let answers = frame(&held, &[&held.issued.lease_id, &under.lease], false).await;
    let breach = log.only("renew_budget_breach");
    drop(log);

    assert_eq!(
        codes(&answers),
        [Some(RUN_BUDGET_EXCEEDED), None],
        "{answers:?}"
    );
    assert_eq!(breach.field("fleet_id"), Some(held.fleet.as_str()));
    assert_eq!(
        breach.field("lease_id"),
        Some(held.issued.lease_id.as_str())
    );
    held.fixtures.cleanup().await;
}

// Value: protects=a short payer's leases stop on a checking tick and run on
// between; fails_when=the check runs every tick or never; why_new=funds are
// checked once per payer every third tick; seam=none; no_claim=the cadence
// itself, which `funds_due`'s unit test proves.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_short_payer_stops_on_a_checking_tick_alone() {
    let held = held().await;
    let funded = held.hold_another().await;
    held.fixtures.seed_wallet(&held.tenant, 0).await;
    let leases = [&held.issued.lease_id, &funded.lease];

    let between = frame(&held, &leases, false).await;
    let log = Capture::install();
    let checking = frame(&held, &leases, true).await;
    let short = log.only("renew_no_credits");
    drop(log);

    assert_eq!(
        codes(&between),
        [None, None],
        "no check between ticks: {between:?}"
    );
    assert_eq!(
        codes(&checking),
        [Some(RUN_LEASE_RENEWAL_NO_CREDITS), None],
        "{checking:?}"
    );
    assert_eq!(short.field("fleet_id"), Some(held.fleet.as_str()));
    assert_eq!(short.field("lease_id"), Some(held.issued.lease_id.as_str()));
    held.fixtures.cleanup().await;
}

// Value: protects=a Kill mark stops its lease alone and keeps its deadline;
// fails_when=the cancel check leaves the statement, or extends the lease
// anyway; why_new=the mark is read in the batch; seam=none; no_claim=the
// mark's delivery, which its own suites prove.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn host_batch_refuses_cancel_marked_lease() {
    let held = held().await;
    let sibling = held.hold_another().await;
    sqlx::query("UPDATE runner.leases SET cancel_requested_at = now() WHERE id = $1")
        .bind(&held.issued.lease_id)
        .execute(&mut *held.fixtures.database.acquire().await.expect("writer"))
        .await
        .expect("the Kill mark writes");
    let before = held
        .fixtures
        .instant(Row::Lease(&held.issued.lease_id), "lease_expires_at")
        .await;

    let log = Capture::install();
    let answers = frame(&held, &[&held.issued.lease_id, &sibling.lease], false).await;
    let refused = log.only("renew_batch_refused");
    drop(log);

    assert_eq!(
        codes(&answers),
        [Some(RUN_LEASE_CANCELLED), None],
        "{answers:?}"
    );
    assert_eq!(refused.field("verdict"), Some("cancelled"));
    assert_eq!(
        refused.field("lease_id"),
        Some(held.issued.lease_id.as_str())
    );
    assert_eq!(refused.field("fleet_id"), Some(held.fleet.as_str()));
    assert_eq!(
        held.fixtures
            .instant(Row::Lease(&held.issued.lease_id), "lease_expires_at")
            .await,
        before,
        "a cancelled lease keeps its deadline"
    );
    held.fixtures.cleanup().await;
}
