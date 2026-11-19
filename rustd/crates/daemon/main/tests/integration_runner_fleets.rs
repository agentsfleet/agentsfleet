//! A running fleet reads which fleets its workspace holds, and what
//! one of them did, through the run it runs.
//!
//! Marked `#[ignore]` like the rest of the live-service suite; run by
//! `make test-integration-rustd`.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "test target: an unmet precondition should fail the test loudly, and a step \
              indexes the JSON it was answered"
)]

use af_core::id::Uuid7;
use agentsfleetd::supervisor::Supervisor;
use serde_json::{Value, json};

use crate::steers::{ARCHIVED, Crew, INSTALLING};
use crate::wire::{report, report_body, wait_for_offer};

/// The ids a page's items name, in order.
fn ids(page: &Value) -> Vec<String> {
    page["items"]
        .as_array()
        .expect("a page carries items")
        .iter()
        .map(|item| {
            item["fleet_id"]
                .as_str()
                .expect("an item names its fleet")
                .to_owned()
        })
        .collect()
}

/// The fleets read under `run`, presenting `fence`.
async fn read_fleets(crew: &Crew, run: &str, fence: u64) -> (u16, Value) {
    crew.read(&format!(
        "/v1/runners/me/runs/{run}/fleets?fencing_token={fence}"
    ))
    .await
}

/// Asserts a read was refused with `status` and the registry code `code`.
fn assert_refused((status, body): (u16, Value), expected: u16, code: &str) {
    assert_eq!(
        (status, &body["error_code"]),
        (expected, &json!(code)),
        "{body}"
    );
}

/// The item of `page` that names `fleet`.
fn item_of<'a>(page: &'a Value, fleet: &Uuid7) -> &'a Value {
    page["items"]
        .as_array()
        .expect("a page carries items")
        .iter()
        .find(|item| item["fleet_id"] == fleet.as_str())
        .expect("the fleet is listed")
}

/// A message waiting for a runner reads as queued, with no answer excerpt.
async fn assert_waiting_message_is_queued(crew: &Crew, run: &str, fence: u64) {
    let waiting = crew
        .run()
        .enqueue_event(af_wire::event::EventType::Chat)
        .await;
    let fleet = &crew.run().fleet;
    let (status, events) = crew
        .read(&format!(
            "/v1/runners/me/runs/{run}/fleets/{fleet}/events?fencing_token={fence}"
        ))
        .await;
    assert_eq!(status, 200, "{events}");
    let queued = events["items"]
        .as_array()
        .expect("items")
        .iter()
        .find(|item| item["event_id"] == waiting.as_str())
        .expect("the waiting message is listed");
    assert_eq!(queued["status"], "queued");
    assert!(queued.get("response_excerpt").is_none(), "{queued}");
}

/// The list names every fleet in the run's workspace, archived and
/// installing ones with their status, and none from another; one fleet's
/// events include a message still waiting for a runner.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn fleets_read_lists_workspace_fleets_for_a_live_run() {
    let mut supervisor = Supervisor::new();
    let crew = Crew::boot(&mut supervisor).await;
    let archived = crew.sibling_as("archived-fleet", ARCHIVED).await;
    let installing = crew.sibling_as("installing-fleet", INSTALLING).await;
    let foreign = crew.foreign("other-workspace-fleet").await;
    let (run, fence) = crew.speaking.run_seeded().await;

    let (status, page) = read_fleets(&crew, &run, fence).await;
    assert_eq!(status, 200, "{page}");
    let listed = ids(&page);
    assert_eq!(listed.len(), 3, "{page}");
    for fleet in [&crew.run().fleet, &archived, &installing] {
        assert!(
            listed.contains(&fleet.to_string()),
            "{fleet} missing: {page}"
        );
    }
    assert!(!listed.contains(&foreign.to_string()), "{page}");
    assert!(page["total"].is_null(), "{page}");
    assert_eq!(item_of(&page, &archived)["status"], "archived");

    assert_waiting_message_is_queued(&crew, &run, fence).await;
    crew.finish(supervisor).await;
}

/// A fleet of another workspace reads as one that does not
/// exist, and a path segment that is not an id is a bad request.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn fleets_read_refuses_a_fleet_from_another_workspace() {
    let mut supervisor = Supervisor::new();
    let crew = Crew::boot(&mut supervisor).await;
    let foreign = crew.foreign("other-workspace-fleet").await;
    let (run, fence) = crew.speaking.run_seeded().await;

    let (status, refused) = crew
        .read(&format!(
            "/v1/runners/me/runs/{run}/fleets/{foreign}/events?fencing_token={fence}"
        ))
        .await;
    assert_eq!(
        (status, &refused["error_code"]),
        (404, &json!("UZ-AGT-009")),
        "{refused}"
    );
    let detail = refused["detail"].as_str().unwrap_or_default();
    assert!(!detail.contains(foreign.as_str()), "{refused}");

    let (status, _refused) = crew
        .read(&format!(
            "/v1/runners/me/runs/{run}/fleets/docs-bot/events?fencing_token={fence}"
        ))
        .await;
    assert_eq!(status, 400);
    crew.finish(supervisor).await;
}

/// A settled run, another runner's run, and a holder a
/// reclaim superseded read nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn fleets_read_refuses_a_run_the_runner_does_not_hold() {
    let mut supervisor = Supervisor::new();
    let crew = Crew::boot(&mut supervisor).await;
    let (settled, fence) = crew.speaking.run_seeded().await;
    let body = report_body(&settled, &crew.run().event_id, fence);
    let reported = report(crew.run(), &body).await;
    assert!(reported.is_ok(), "the seeded run settles: {reported:?}");
    assert_refused(read_fleets(&crew, &settled, fence).await, 404, "UZ-RUN-006");

    let unknown = "0190d5c2-7e1a-7000-8000-00000000dead";
    assert_refused(read_fleets(&crew, unknown, fence).await, 404, "UZ-RUN-006");

    let next = crew
        .run()
        .enqueue_event(af_wire::event::EventType::Chat)
        .await;
    let (live, fence) = wait_for_offer(crew.run(), &next).await;
    crew.speaking.supersede().await;
    let answered = read_fleets(&crew, &live, fence).await;
    assert_eq!(answered.1["current_state"], "superseded", "{}", answered.1);
    assert_refused(answered, 409, "UZ-RUN-005");
    crew.finish(supervisor).await;
}
