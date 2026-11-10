//! A committed admission stays accepted even when every notification fails.

use super::{
    Fallible,
    lane::Lane,
    queue::{RING_FAILED, admit_against, confirm, over, over_bus, recording},
};

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn notification_failure_cannot_downgrade_committed_acceptance() -> Fallible {
    let lane = Lane::seed().await?;
    let admissions = over(&lane, afd_bus::test_util::unreachable());
    let first = admit_against(&lane, &admissions).await;
    let id = first.admitted()?.stored.id.clone();
    let confirmed = recording(async {
        let once = confirm(&admissions, &lane).await?;
        let again = confirm(&admissions, &lane).await?;
        Fallible::<_>::Ok((once, again))
    })
    .await;
    assert_eq!(
        confirmed.count(RING_FAILED),
        2,
        "each confirmation rang, and each lost ring was logged"
    );
    let (once, again) = confirmed.outcome?;
    assert_eq!(once.event_id, id);
    assert_eq!(again, once);
    lane.cleanup().await;
    Ok(())
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn confirming_an_absent_key_does_not_publish_or_create_work() -> Fallible {
    let lane = Lane::seed().await?;
    let (mut rings, admissions) = over_bus(&lane).await?;
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
    assert_eq!(rings.taken().await?, Vec::<String>::new());
    lane.cleanup().await;
    Ok(())
}
