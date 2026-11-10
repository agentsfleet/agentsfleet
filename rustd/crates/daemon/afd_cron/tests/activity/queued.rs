//! Queued fires reload before leasing and promote under their original identity.

use af_core::event::status;
use af_core::id::Uuid7;
use afd_cron::{Change, Fire};
use serde_json::{Value, json};

use super::fixture::*;
use crate::support::CronLane;

const DELIVER: &str = "WITH landed AS (INSERT INTO fleet.events \
    (fleet_id,workspace_id,event_id,actor,event_type,status,request_json,created_at,updated_at) \
    SELECT fleet_id,workspace_id,id,actor,event_type,$2,request_json::jsonb,event_created_at,now() \
    FROM fleet.admissions WHERE id=$1 RETURNING event_id) \
    UPDATE fleet.admissions SET delivered_at=now() WHERE id IN (SELECT event_id FROM landed)";

// Value: protects=one accepted fire is readable offline and remains one row after delivery;
// fails_when=queued history excludes schedule_fire or retry rewrites its first payload;
// seam=atomic SQL seeds the reachable delivered state; no_claim=signatures or native lease execution.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn queued_schedule_fire_history_survives_reload_and_single_row_promotion() {
    let lane = CronLane::open().await;
    let row = registered(&lane).await;
    let id = &row.schedule_id;
    let store = &lane.store;
    let target = store.fire_target(id).await.expect("read").expect("row");
    let fire = Fire::new(lane.admissions());
    let accepted = fire
        .deliver(id, &target, "first-fire")
        .await
        .expect("admit");
    let queued = cron_rows(&lane).await;
    assert_eq!(queued.len(), 1);
    let queued = queued.first().expect("offline fire");
    assert_eq!(queued.row.event_id, accepted.event_id);
    assert_eq!(queued.row.status, status::QUEUED);
    let payload: Value = serde_json::from_str(&queued.request_json).expect("snapshot");
    assert_eq!(
        payload,
        json!({"message":MESSAGE,"schedule":{"schedule_id":id.as_str(),"description":DESCRIPTION}})
    );
    let update = Change {
        message: Some("Later support message"),
        cron: Some("0 10 * * *"),
        ..Change::default()
    };
    change(&lane, &row, update, &CronLane::token()).await;
    let target = store.fire_target(id).await.expect("read").expect("row");
    let repeated = fire
        .deliver(id, &target, "first-fire")
        .await
        .expect("replay");
    assert_eq!(repeated.event_id, accepted.event_id);
    assert!(repeated.replayed);
    let event = Uuid7::parse(&accepted.event_id).expect("a fire answers a minted event id");
    sqlx::query(DELIVER)
        .bind(&event)
        .bind(status::RECEIVED)
        .execute(&mut *lane.connection().await)
        .await
        .expect("atomic delivered-state fixture");
    let delivered = cron_rows(&lane).await;
    assert_eq!(delivered.len(), 1);
    let delivered = delivered.first().expect("single promoted row");
    assert_eq!(delivered.row.event_id, accepted.event_id);
    assert_eq!(delivered.row.status, status::RECEIVED);
    assert_eq!(
        serde_json::from_str::<Value>(&delivered.request_json).expect("retained payload"),
        payload
    );
}

async fn cron_rows(lane: &CronLane) -> Vec<afd_events::EventDetailRow> {
    thread(lane)
        .await
        .into_iter()
        .filter(|row| row.row.event_type == "cron")
        .collect()
}
