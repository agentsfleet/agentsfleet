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
