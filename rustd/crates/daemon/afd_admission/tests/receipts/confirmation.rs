//! A committed admission stays accepted even when every notification fails.

use super::{
    Fallible,
    lane::Lane,
    queue::{admit_against, confirm, over_fake},
};
use crate::fake_redis::Reply;

const REFUSED: &str = "-ERR notification unavailable\r\n";

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn notification_failure_cannot_downgrade_committed_acceptance() -> Fallible {
    let lane = Lane::seed().await?;
    let first = admit_against(&lane, &[("SPUBLISH", Reply::Raw(REFUSED))]).await?;
    let id = first.admitted()?.stored.id.clone();
    let (server, admissions) = over_fake(&lane, &[("SPUBLISH", Reply::Raw(REFUSED))]).await?;
    let confirmed = confirm(&admissions, &lane).await?;
    assert_eq!(confirmed.event_id, id);
    assert_eq!(confirm(&admissions, &lane).await?, confirmed);
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
    Ok(())
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn confirming_an_absent_key_does_not_publish_or_create_work() -> Fallible {
    let lane = Lane::seed().await?;
    let (server, admissions) = over_fake(&lane, &[]).await?;
    assert_eq!(
        admissions
            .confirm(afd_admission::Producer::Steer, lane.fleet.as_str())
            .await?,
        None
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM fleet.admissions WHERE fleet_id=$1")
        .bind(&lane.fleet)
        .fetch_one(&mut *lane.database.acquire().await?)
        .await?;
    assert_eq!(count, 0);
    assert!(!server.seen().iter().any(|s| s.starts_with("SPUBLISH")));
    lane.cleanup().await;
    Ok(())
}
