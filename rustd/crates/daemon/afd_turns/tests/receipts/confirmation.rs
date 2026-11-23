//! A committed turn stays accepted even when every notification fails.

use super::{
    Fallible,
    lane::Lane,
    queue::{RING_FAILED, admit_against, confirm, over, over_bus, recording},
};

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn notification_failure_cannot_downgrade_committed_acceptance() -> Fallible {
    let lane = Lane::seed().await?;
    let turns = over(&lane, afd_bus::test_util::unreachable());
    let first = admit_against(&lane, &turns).await;
    let id = first.admitted()?.stored.id.clone();
    let confirmed = recording(async {
        let once = confirm(&turns, &lane).await?;
        let again = confirm(&turns, &lane).await?;
        Fallible::<_>::Ok((once, again))
    })
    .await;
    assert_eq!(
        confirmed.count(RING_FAILED),
        2,
        "each confirmation rang, and each lost ring was logged"
    );
    let (once, again) = confirmed.outcome?;
    assert_eq!(once.turn_id, id);
    assert_eq!(again, once);
    lane.cleanup().await;
    Ok(())
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn confirming_an_absent_key_does_not_publish_or_create_work() -> Fallible {
    let lane = Lane::seed().await?;
    let (mut rings, turns) = over_bus(&lane).await?;
    assert_eq!(
        turns
            .confirm(afd_turns::Producer::Steer, lane.fleet.as_str())
            .await?,
        None
    );
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM thread.turns WHERE producer IS NOT NULL AND thread_id = $1",
    )
    .bind(&lane.fleet)
    .fetch_one(&mut *lane.database.acquire().await?)
    .await?;
    assert_eq!(count, 0);
    assert_eq!(rings.taken().await?, Vec::<String>::new());
    lane.cleanup().await;
    Ok(())
}
