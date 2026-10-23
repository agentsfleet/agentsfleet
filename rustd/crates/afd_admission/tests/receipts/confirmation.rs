//! A committed admission stays accepted even when every notification fails.

use super::{
    lane::Lane,
    queue::{admit_against, confirm, over_fake},
};
use crate::fake_redis::Reply;

const REFUSED: &str = "-ERR notification unavailable\r\n";

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn notification_failure_cannot_downgrade_committed_acceptance() {
    let lane = Lane::seed().await;
    let first = admit_against(&lane, &[("SPUBLISH", Reply::Raw(REFUSED))]).await;
    let id = first.outcome.expect("committed acceptance").stored.id;
    let (server, admissions) = over_fake(&lane, &[("SPUBLISH", Reply::Raw(REFUSED))]).await;
    let confirmed = confirm(&admissions, &lane).await.expect("confirmation");
    assert_eq!(confirmed.event_id, id);
    assert_eq!(confirm(&admissions, &lane).await.expect("retry"), confirmed);
    assert_eq!(
        server
            .seen()
            .iter()
            .filter(|s| s.starts_with("SPUBLISH"))
            .count(),
        2
    );
    assert!(
        !server
            .seen()
            .iter()
            .any(|s| s.starts_with("XADD") || s.starts_with("HSET"))
    );
    lane.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn confirming_an_absent_key_does_not_publish_or_create_work() {
    let lane = Lane::seed().await;
    let (server, admissions) = over_fake(&lane, &[]).await;
    assert_eq!(
        admissions
            .confirm(afd_admission::Producer::Steer, &lane.fleet)
            .await
            .expect("lookup"),
        None
    );
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM fleet.admissions WHERE fleet_id=$1::uuid")
            .bind(&lane.fleet)
            .fetch_one(&mut *lane.database.acquire().await.expect("database"))
            .await
            .expect("row count");
    assert_eq!(count, 0);
    assert!(!server.seen().iter().any(|s| s.starts_with("SPUBLISH")));
    lane.cleanup().await;
}
