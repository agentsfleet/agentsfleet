//! Notification errors leave accepted work durable; rejected admission never publishes.

use super::{
    Fallible,
    lane::Lane,
    ledger::{admit, steer},
    queue::{RING_FAILED, admit_against, over, over_bus, silent},
};
use af_core::{error_code, test_util::trace::assert_uncoded};
use afd_admission::Reply as ReplyTo;

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_refused_notification_is_logged_and_leaves_one_queued_row() -> Fallible {
    let lane = Lane::seed().await?;
    let run = admit_against(&lane, &over(&lane, afd_bus::test_util::unreachable())).await;
    assert!(!run.admitted()?.replayed);
    let logged = run
        .event(RING_FAILED)
        .ok_or("the lost notification is logged")?;
    assert_eq!(
        logged.get("error_code").map(String::as_str),
        Some(error_code::INTERNAL_OPERATION_FAILED.as_str())
    );
    assert_uncoded(
        logged.get("reason").map_or("", String::as_str),
        "notification failure",
    );
    let queued: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM fleet.admissions WHERE fleet_id=$1 AND settled_at IS NULL",
    )
    .bind(&lane.fleet)
    .fetch_one(&mut *lane.database.acquire().await?)
    .await?;
    assert_eq!(queued, 1);
    lane.cleanup().await;
    Ok(())
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_silent_notification_is_bounded_and_does_not_lose_acceptance() -> Fallible {
    let lane = Lane::seed().await?;
    let run = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        admit_against(&lane, &over(&lane, silent())),
    )
    .await?;
    let admitted = run.admitted()?;
    let logged = run.event(RING_FAILED).ok_or("the deadline is logged")?;
    assert_uncoded(
        logged.get("reason").map_or("", String::as_str),
        "notification timeout",
    );
    let (mut rings, admissions) = over_bus(&lane).await?;
    let replayed = admit(
        &admissions,
        steer(&lane, lane.fleet.as_str(), ReplyTo::None),
    )
    .await?;
    assert!(replayed.replayed);
    assert_eq!(replayed.stored.id, admitted.stored.id);
    assert_eq!(rings.taken().await?, [lane.fleet.as_str()]);
    lane.cleanup().await;
    Ok(())
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn an_invalid_fleet_scope_records_nothing_and_sends_no_notification() -> Fallible {
    let lane = Lane::seed().await?;
    let (mut rings, admissions) = over_bus(&lane).await?;
    let wrong = afd_db::test_util::mint_id();
    let mut request = steer(&lane, lane.fleet.as_str(), ReplyTo::None);
    request.workspace = &wrong;
    let refused = admissions
        .admit(request)
        .await
        .err()
        .ok_or("a fleet of another workspace is refused")?;
    assert!(refused.is_fleet_absent(), "{refused}");
    assert!(!refused.is_over_capacity());
    assert_eq!(rings.taken().await?, Vec::<String>::new());
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM fleet.admissions WHERE fleet_id=$1")
        .bind(&lane.fleet)
        .fetch_one(&mut *lane.database.acquire().await?)
        .await?;
    assert_eq!(count, 0);
    lane.cleanup().await;
    Ok(())
}
