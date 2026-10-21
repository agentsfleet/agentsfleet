//! Notification errors leave accepted work durable; rejected admission never publishes.

use super::{
    lane::Lane,
    ledger::{admit, steer},
    queue::{admit_against, over_fake},
};
use crate::fake_redis::Reply;
use afd_admission::Reply as ReplyTo;
use afd_core::{error_code, test_util::trace::assert_uncoded};

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_refused_notification_is_logged_and_leaves_one_queued_row() {
    let lane = Lane::seed().await;
    let run = admit_against(
        &lane,
        &[("SPUBLISH", Reply::Raw("-ERR publish refused\r\n"))],
    )
    .await;
    assert!(!run.outcome.as_ref().expect("accepted row").replayed);
    let logged = run
        .event("admission_wake_ring_failed")
        .expect("lost notification logged");
    assert_eq!(
        logged.get("error_code").map(String::as_str),
        Some(error_code::INTERNAL_OPERATION_FAILED.as_str())
    );
    assert_uncoded(
        logged.get("reason").map_or("", String::as_str),
        "notification failure",
    );
    let queued: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM fleet.admissions WHERE fleet_id=$1::uuid AND settled_at IS NULL",
    )
    .bind(&lane.fleet)
    .fetch_one(&mut *lane.database.acquire().await.expect("database"))
    .await
    .expect("queue count");
    assert_eq!(queued, 1);
    assert!(
        !run.seen
            .iter()
            .any(|s| s.starts_with("XADD") || s.starts_with("HSET"))
    );
    lane.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_silent_notification_is_bounded_and_does_not_lose_acceptance() {
    let lane = Lane::seed().await;
    let run = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        admit_against(&lane, &[("SPUBLISH", Reply::Silent)]),
    )
    .await
    .expect("notification deadline bounds acceptance");
    let admitted = run.outcome.as_ref().expect("the commit stands");
    let logged = run
        .event("admission_wake_ring_failed")
        .expect("deadline logged");
    assert_uncoded(
        logged.get("reason").map_or("", String::as_str),
        "notification timeout",
    );
    let (server, admissions) = over_fake(&lane, &[]).await;
    let replayed = admit(&admissions, steer(&lane, &lane.fleet, ReplyTo::None)).await;
    assert!(replayed.replayed);
    assert_eq!(replayed.stored.id, admitted.stored.id);
    assert_eq!(
        server
            .seen()
            .iter()
            .filter(|s| s.starts_with("SPUBLISH"))
            .count(),
        1
    );
    lane.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn an_invalid_fleet_scope_records_nothing_and_sends_no_notification() {
    let lane = Lane::seed().await;
    let (server, admissions) = over_fake(&lane, &[]).await;
    let wrong = afd_db::test_util::mint_id();
    let mut request = steer(&lane, &lane.fleet, ReplyTo::None);
    request.workspace = &wrong;
    let refused = admissions
        .admit(request)
        .await
        .expect_err("fleet belongs to another workspace");
    assert!(!refused.is_over_capacity());
    assert!(!server.seen().iter().any(|s| s.starts_with("SPUBLISH")));
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM fleet.admissions WHERE fleet_id=$1::uuid")
            .bind(&lane.fleet)
            .fetch_one(&mut *lane.database.acquire().await.expect("database"))
            .await
            .expect("rejected row count");
    assert_eq!(count, 0);
    lane.cleanup().await;
}
