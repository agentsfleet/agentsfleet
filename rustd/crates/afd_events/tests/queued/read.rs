//! Queued and delivered messages keep one logical identity.

use super::*;

/// Dimension 2.1: an admitted, undelivered steer leads the first page as a
/// waiting row carrying its body, and a later page does not repeat it.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_thread_read_includes_queued_steers() {
    let lane = EventsLane::open().await;
    let steered = Steer::new(lane.admissions())
        .append(&lane.fleet, &lane.workspace, ACTOR, BODY, None)
        .await
        .expect("the steer is admitted");
    let history = History::new(lane.database.clone());
    let (workspace, fleet) = (uuid(&lane.workspace), uuid(&lane.fleet));

    let page = history
        .thread_page(&workspace, &fleet, None, PAGE)
        .await
        .expect("the thread reads");
    let first = page.first().expect("the waiting message is on the page");
    assert_eq!(first.row.event_id, steered.event_id);
    assert_eq!(first.row.status, status::QUEUED);
    assert_eq!(first.row.actor, ACTOR);
    assert_eq!(first.request_json, BODY);
    assert_eq!(first.response_text, None);
    assert_eq!(first.task_activity.items, []);
    assert!(!first.task_activity.truncated);

    lane.cleanup().await;
}

/// Dimension 2.2: once a runner has it, the event is one history row and no
/// longer waiting.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_thread_read_dedupes_leased_steer() {
    let lane = EventsLane::open().await;
    let steered = Steer::new(lane.admissions())
        .append(&lane.fleet, &lane.workspace, ACTOR, BODY, None)
        .await
        .expect("the steer is admitted");
    let admitted_at = steered
        .admitted_at
        .expect("a fresh admission has an instant");
    let mut connection = lane.connection().await;
    sqlx::query(MARK_DELIVERED)
        .bind(lane.fleet.as_str())
        .execute(&mut *connection)
        .await
        .expect("the lease's delivery stamp lands");
    drop(connection);
    lane.seed_event(&steered.event_id, admitted_at).await;

    let page = History::new(lane.database.clone())
        .thread_page(&uuid(&lane.workspace), &uuid(&lane.fleet), None, PAGE)
        .await
        .expect("the thread reads");
    let rows: Vec<_> = page
        .iter()
        .filter(|row| row.row.event_id == steered.event_id)
        .collect();
    assert_eq!(rows.len(), 1, "one event, one row");
    assert_ne!(
        rows.first().map(|row| row.row.status.as_str()),
        Some(status::QUEUED)
    );

    lane.cleanup().await;
}

/// The storage array and truncation scalar reconstruct the existing public history.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn task_history_preserves_records_and_truncation_in_detail_and_thread() {
    let lane = EventsLane::open().await;
    let event = "01924f4e-0000-7000-8000-000000000236";
    lane.seed_event(event, jiff::Timestamp::now()).await;
    let items = serde_json::json!([{
        "kind": "task_decision_started",
        "progress": {
            "activity_id": event, "task_id": null, "source_admission_id": null,
            "operation": "intent", "phase": "starting", "provider": null,
            "model": null, "completed": 0, "total": null, "sequence": 2
        }
    }]);
    let mut connection = lane.connection().await;
    sqlx::query(
        "UPDATE fleet.events SET task_activity = $3, task_activity_sequence = 2, \
                 task_activity_truncated = true WHERE fleet_id = $1::uuid AND event_id = $2",
    )
    .bind(lane.fleet.as_str())
    .bind(event)
    .bind(&items)
    .execute(&mut *connection)
    .await
    .expect("saved history fixture");
    drop(connection);
    let history = History::new(lane.database.clone());
    let (workspace, fleet) = (uuid(&lane.workspace), uuid(&lane.fleet));
    let detail = history
        .detail(&workspace, &fleet, event)
        .await
        .expect("detail decodes split storage")
        .expect("event exists");
    let page = history
        .thread_page(&workspace, &fleet, None, PAGE)
        .await
        .expect("thread decodes split storage");
    let thread = page
        .iter()
        .find(|row| row.row.event_id == event)
        .expect("saved event appears in thread");
    assert_eq!(detail.task_activity, thread.task_activity);
    assert!(detail.task_activity.truncated);
    assert_eq!(
        serde_json::to_value(&detail.task_activity.items).expect("public items serialize"),
        items
    );
    lane.cleanup().await;
}
