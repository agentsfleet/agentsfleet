//! Explicit schedule inputs and public history reads shared by these scenarios.

use af_core::id::Uuid7;
use afd_cron::{Change, NewSchedule, Schedule, Source};
use afd_events::{EventDetailRow, History};
use serde_json::{Value, json};

use crate::support::{CronLane, SEED_AT};

pub(super) const MESSAGE: &str = "Review support failures — 東京 🦉";
pub(super) const DESCRIPTION: &str = "At 9:00 AM (Asia/Kolkata)";
pub(super) const EXPRESSION: &str = "0 9 * * *";
pub(super) const ZONE: &str = "Asia/Kolkata";

pub(super) fn new(fleet: &Uuid7) -> NewSchedule<'_> {
    NewSchedule {
        fleet,
        source: Source::Api,
        provider_schedule_id: None,
        cron: EXPRESSION,
        timezone: ZONE,
        message: MESSAGE,
        once: false,
    }
}

pub(super) async fn create(lane: &CronLane, token: &Uuid7) -> Schedule {
    lane.store
        .create(&lane.workspace_id(), new(&lane.fleet_id()), token, SEED_AT)
        .await
        .expect("create transaction")
        .expect("native input accepted")
}

pub(super) async fn registered(lane: &CronLane) -> Schedule {
    let token = CronLane::token();
    let row = create(lane, &token).await;
    confirm(lane, &row, &token).await
}

pub(super) async fn confirm(lane: &CronLane, row: &Schedule, token: &Uuid7) -> Schedule {
    lane.store
        .finalize_synced(row, token, None)
        .await
        .expect("native confirmation")
        .expect("current claim")
}

pub(super) async fn change(
    lane: &CronLane,
    row: &Schedule,
    change: Change<'_>,
    token: &Uuid7,
) -> Schedule {
    lane.store
        .claim_change(&lane.fleet_id(), &row.schedule_id, change, token)
        .await
        .expect("native mutation")
        .expect("current claim")
}

pub(super) async fn stored(lane: &CronLane, id: &Uuid7) -> Option<Schedule> {
    lane.store
        .one(&lane.fleet_id(), id)
        .await
        .expect("stored schedule")
}

pub(super) async fn thread(lane: &CronLane) -> Vec<EventDetailRow> {
    History::new(lane.database.clone())
        .thread_page(
            &lane.workspace_id(),
            &lane.fleet_id(),
            None,
            afd_events::THREAD_MAX_LIMIT,
        )
        .await
        .expect("reload native fleet history")
}

pub(super) async fn receipts(lane: &CronLane) -> Vec<Value> {
    thread(lane)
        .await
        .into_iter()
        .filter_map(|event| {
            if event.row.event_type != afd_events::activity::EVENT_TYPE_ACTIVITY {
                return None;
            }
            assert_eq!(
                (event.row.actor.as_str(), event.row.status.as_str()),
                ("system", "recorded")
            );
            assert_eq!((event.response_text, event.tool_calls), (None, None));
            Some(serde_json::from_str(&event.request_json).expect("typed receipt JSON"))
        })
        .collect()
}

pub(super) fn fact(id: &Uuid7, kind: &str, status: &str, sync: &str, message: &str) -> Value {
    json!({"kind":kind,"schedule_id":id.as_str(),"description":DESCRIPTION,
        "message":message,"status":status,"sync":sync})
}

pub(super) fn changed(id: &Uuid7, change: &str, status: &str, message: &str) -> Value {
    json!({"kind":"schedule_changed","change":change,"schedule_id":id.as_str(),
        "description":DESCRIPTION,"message":message,"status":status,"sync":"syncing"})
}

pub(super) fn same_facts(mut actual: Vec<Value>, mut expected: Vec<Value>) {
    actual.sort_by_key(Value::to_string);
    expected.sort_by_key(Value::to_string);
    assert_eq!(
        actual, expected,
        "complete snapshots, without extra or missing receipts"
    );
}

pub(super) async fn no_work(lane: &CronLane) {
    let mut connection = lane.connection().await;
    let counts: (i64, i64) = sqlx::query_as(
        "SELECT \
        (SELECT count(*) FROM fleet.admissions WHERE fleet_id=$1::uuid), \
        (SELECT count(*) FROM fleet.tasks WHERE fleet_id=$1::uuid)",
    )
    .bind(lane.fleet.as_str())
    .fetch_one(&mut *connection)
    .await
    .expect("work ownership");
    assert_eq!(
        counts,
        (0, 0),
        "activity admits neither a run nor a Durable Task"
    );
}
