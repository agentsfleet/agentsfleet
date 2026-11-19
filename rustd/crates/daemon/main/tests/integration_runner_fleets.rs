//! M252 §1 — a running fleet reads which fleets its workspace holds, and what
//! one of them did, through the lease it runs.
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

/// Dimension 1.1. The list names every fleet in the lease's workspace,
/// archived and installing ones with their status, and none from another;
/// one fleet's events include a message still waiting for a runner.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn fleets_read_lists_workspace_fleets_for_a_live_lease() {
    let mut supervisor = Supervisor::new();
    let crew = Crew::boot(&mut supervisor).await;
    let archived = crew.sibling_as("archived-fleet", ARCHIVED).await;
    let installing = crew.sibling_as("installing-fleet", INSTALLING).await;
    let foreign = crew.foreign("other-workspace-fleet").await;
    let (lease, fence) = crew.speaking.lease_seeded().await;

    let (status, page) = crew
        .read(&format!(
            "/v1/runners/me/leases/{lease}/fleets?fencing_token={fence}"
        ))
        .await;
    assert_eq!(status, 200, "{page}");
    let listed = ids(&page);
    assert_eq!(listed.len(), 3, "{page}");
    for fleet in [&crew.run().fleet, &archived, &installing] {
        assert!(
            listed.contains(&fleet.as_str().to_owned()),
            "{fleet} missing: {page}"
        );
    }
    assert!(!listed.contains(&foreign.as_str().to_owned()), "{page}");
    assert!(page["total"].is_null(), "{page}");
    let archived_item = page["items"]
        .as_array()
        .expect("items")
        .iter()
        .find(|item| item["fleet_id"] == archived.as_str())
        .expect("the archived fleet is listed");
    assert_eq!(archived_item["status"], "archived");

    let waiting = crew
        .run()
        .enqueue_event(af_wire::event::EventType::Chat)
        .await;
    let fleet = &crew.run().fleet;
    let (status, events) = crew
        .read(&format!(
            "/v1/runners/me/leases/{lease}/fleets/{fleet}/events?fencing_token={fence}"
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
    crew.finish(supervisor).await;
}

/// Dimension 1.2. A fleet of another workspace reads as one that does not
/// exist, and a path segment that is not an id is a bad request.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn fleets_read_refuses_a_fleet_from_another_workspace() {
    let mut supervisor = Supervisor::new();
    let crew = Crew::boot(&mut supervisor).await;
    let foreign = crew.foreign("other-workspace-fleet").await;
    let (lease, fence) = crew.speaking.lease_seeded().await;

    let (status, refused) = crew
        .read(&format!(
            "/v1/runners/me/leases/{lease}/fleets/{foreign}/events?fencing_token={fence}"
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
            "/v1/runners/me/leases/{lease}/fleets/docs-bot/events?fencing_token={fence}"
        ))
        .await;
    assert_eq!(status, 400);
    crew.finish(supervisor).await;
}

/// Dimension 1.3. A settled lease, another runner's lease, and a holder a
/// reclaim superseded read nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn fleets_read_refuses_a_lease_the_runner_does_not_hold() {
    let mut supervisor = Supervisor::new();
    let crew = Crew::boot(&mut supervisor).await;
    let (settled, fence) = crew.speaking.lease_seeded().await;
    let reported = report(
        crew.run(),
        &report_body(&settled, &crew.run().event_id, fence),
    )
    .await;
    assert!(reported.is_ok(), "the seeded lease settles: {reported:?}");
    let (status, refused) = crew
        .read(&format!(
            "/v1/runners/me/leases/{settled}/fleets?fencing_token={fence}"
        ))
        .await;
    assert_eq!(
        (status, &refused["error_code"]),
        (404, &json!("UZ-RUN-006")),
        "{refused}"
    );

    let unknown = "0190d5c2-7e1a-7000-8000-00000000dead";
    let (status, refused) = crew
        .read(&format!(
            "/v1/runners/me/leases/{unknown}/fleets?fencing_token={fence}"
        ))
        .await;
    assert_eq!(
        (status, &refused["error_code"]),
        (404, &json!("UZ-RUN-006")),
        "{refused}"
    );

    let next = crew
        .run()
        .enqueue_event(af_wire::event::EventType::Chat)
        .await;
    let (live, fence) = wait_for_offer(crew.run(), &next).await;
    crew.speaking.supersede().await;
    let (status, refused) = crew
        .read(&format!(
            "/v1/runners/me/leases/{live}/fleets?fencing_token={fence}"
        ))
        .await;
    assert_eq!(
        (status, &refused["error_code"], &refused["current_state"]),
        (409, &json!("UZ-RUN-005"), &json!("superseded")),
        "{refused}"
    );
    crew.finish(supervisor).await;
}
