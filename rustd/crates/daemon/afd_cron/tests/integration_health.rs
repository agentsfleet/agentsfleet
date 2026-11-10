//! One binary owns this snapshot walk, so no concurrent suite mutates its counts.
#![expect(clippy::expect_used, reason = "live store fixtures fail loudly")]
#[path = "support/cron_lane.rs"]
mod support;

use af_core::id::Uuid7;
use afd_cron::ScheduleService;
use jiff::Timestamp;
use jiff_sqlx::ToSqlx as _;
use support::CronLane;

fn change(after: [u64; 7], before: [u64; 7]) -> Vec<i128> {
    after
        .into_iter()
        .zip(before)
        .map(|(a, b)| i128::from(a) - i128::from(b))
        .collect()
}

#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn health_tracks_real_states_and_refuses_a_closed_database() {
    let lane = CronLane::open().await;
    let now = Timestamp::now();
    let before = lane.store.health(now).await.expect("initial real snapshot");
    let row = lane.create("health-walk", "0 3 * * *").await;
    let after = lane.store.health(now).await.expect("created snapshot");
    assert_eq!(change(after, before), [1, 0, 0, 0, 1, 0, 0]);
    let mut connection = lane.database.acquire().await.expect("fixture connection");
    sqlx::query(
        "UPDATE fleet.schedules SET desired_status='paused', sync_status='failed' WHERE id=$1",
    )
    .bind(&row.schedule_id)
    .execute(&mut *connection)
    .await
    .expect("owned state change");
    let paused = lane.store.health(now).await.expect("paused snapshot");
    assert_eq!(change(paused, after), [-1, 1, 0, 0, -1, 1, 0]);
    sqlx::query(
        "UPDATE fleet.schedules SET desired_status='deleting', sync_status='synced' WHERE id=$1",
    )
    .bind(&row.schedule_id)
    .execute(&mut *connection)
    .await
    .expect("owned deleting state");
    let deleting = lane.store.health(now).await.expect("deleting snapshot");
    assert_eq!(change(deleting, paused), [0, -1, 1, 1, 0, -1, 0]);
    sqlx::query("UPDATE fleet.schedules SET desired_status='active', once=true, fire_at=$2::timestamptz - interval '1 second' WHERE id=$1")
        .bind(&row.schedule_id).bind(now.to_sqlx()).execute(&mut *connection).await.expect("owned overdue one-shot");
    let overdue = lane.store.health(now).await.expect("overdue snapshot");
    assert_eq!(change(overdue, deleting), [1, 0, -1, 0, 0, 0, 1]);
    drop(connection);
    let held = lane
        .store
        .one(&lane.fleet_id(), &row.schedule_id)
        .await
        .expect("owned row reads")
        .expect("owned row exists");
    #[expect(
        clippy::disallowed_methods,
        reason = "the test scheduler is loopback and the closed database must prevent any send"
    )]
    let client = reqwest::Client::new();
    let upstream = afd_cron::qstash::QStash::new(
        client,
        af_crypto::secret::SecretString::new("fixture-token".into()),
        "https://example.test/fire".into(),
        "http://127.0.0.1:1".into(),
    );
    let service = ScheduleService::new(lane.store.clone(), upstream);
    lane.database.close().await;
    lane.store
        .health(now)
        .await
        .expect_err("a closed database cannot yield a snapshot");
    let token = Uuid7::parse("01900000-0000-7000-8000-000000000001").expect("canonical token");
    service
        .reconcile(&held, &token)
        .await
        .expect_err("reconciliation must retain the database failure");
}
