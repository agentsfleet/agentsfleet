//! Waiting admissions participate in every cursor page.

use super::*;

/// More messages wait than one page holds: walking the thread page by page,
/// the way the route cuts and resumes it, reaches every one of them, newest
/// first, each once. Waiting rows used to join the first page only, so the
/// ones it cut never came back until a runner took them.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_waiting_overflow_reaches_later_pages() {
    let lane = EventsLane::open().await;
    let steer = Steer::new(lane.admissions());
    let mut admitted = Vec::with_capacity(WAITING);
    for _ in 0..WAITING {
        let steered = steer
            .append(&lane.fleet, &lane.workspace, ACTOR, BODY, None)
            .await
            .expect("the steer is admitted");
        admitted.push(steered.event_id);
    }
    admitted.reverse();
    let history = History::new(lane.database.clone());
    let (workspace, fleet) = (uuid(&lane.workspace), uuid(&lane.fleet));

    let mut walked = Vec::new();
    let mut cursor: Option<Cursor> = None;
    let page_rows = usize::try_from(SMALL_PAGE).expect("a page size is positive");
    loop {
        let page = history
            .thread_page(&workspace, &fleet, cursor.as_ref(), SMALL_PAGE)
            .await
            .expect("the thread reads");
        let kept: Vec<_> = page.into_iter().take(page_rows).collect();
        let Some(last) = kept.last() else { break };
        cursor = Some(Cursor::after(last.row.created_at, &last.row.event_id));
        let full = kept.len() == page_rows;
        walked.extend(kept.into_iter().map(|row| row.row.event_id));
        if !full {
            break;
        }
    }
    assert_eq!(
        walked, admitted,
        "every waiting message, newest first, once"
    );

    clean(&lane, &FleetStreams::new(lane.queue.clone())).await;
    lane.cleanup().await;
}
