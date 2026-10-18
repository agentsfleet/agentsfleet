//! Live daemon coverage for an operator label edit reaching placement.
//!
//! The runner binary never reads its own labels: the daemon's lease poll reads
//! `fleet.runners.labels` afresh on every request and offers a fleet only when
//! its `required_tags` are a subset of them. So an edit takes effect on the
//! runner's next polls, with no restart, no re-enrolment and nothing for the
//! host to pick up, and this is the test that holds the daemon to that.

use super::*;

use crate::e2e::EVENT_TYPE;
use afd_runner::{RunnerEdit, Runners};
use afd_wire::runner::AssignedPolicy;
use std::borrow::Cow;

/// The label enrolment gave the scenario's runner (`e2e_seed::enrolment`).
const ENROLLED_LABEL: &str = "fixture";

/// Pins the scenario's fleet to `tag`, the tag no runner in the lane carries.
async fn require_tag(run: &Scenario, tag: &str) {
    let mut connection = run.booted.database.acquire().await.expect("a connection");
    sqlx::query("UPDATE fleet.fleets SET required_tags = ARRAY[$2::text] WHERE id = $1::uuid")
        .bind(&run.fleet)
        .bind(tag)
        .execute(&mut *connection)
        .await
        .expect("the fleet's required tags update");
}

/// Replaces the runner's labels through the store verb the operator's
/// `PATCH /v1/fleets/runners/{runner_id}` calls, and checks what it stored.
async fn edit_labels(run: &Scenario, labels: &[&str]) {
    let labels: Vec<Cow<'_, str>> = labels.iter().map(|label| Cow::Borrowed(*label)).collect();
    let edit = RunnerEdit {
        assigned_policy: None::<&AssignedPolicy<'_>>,
        labels: Some(&labels),
    };
    let stored = Runners::new(
        run.booted.database.clone(),
        afd_crypto::entropy::Entropy::new(),
    )
    .edit(&run.runner_id, edit, afd_core::clock::now())
    .await
    .expect("an operator may edit a live runner's labels");
    assert_eq!(stored.labels.as_deref(), Some(labels.as_slice()));
}

/// Spends the whole poll budget asserting the fleet under test is never offered.
async fn assert_never_offered(http: &reqwest::Client, run: &Scenario, why: &str) {
    let ended = poll_until(http, run, || async { false }).await;
    assert!(!ended, "{why}");
}

/// A fleet requiring a tag is withheld from a runner without it, offered on
/// that same runner's next polls once an operator adds the label, and withheld
/// again once the label is edited away — with the runner never restarted or
/// re-enrolled.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_operator_label_edit_changes_placement_on_the_next_poll() {
    let mut supervisor = Supervisor::new();
    let run = scenario(&mut supervisor).await;
    let http = reqwest::Client::new();
    // `gpu`, made unique to this fleet so another suite's runner on the shared
    // lane can never be the one that matches it.
    let tag = format!("gpu-{}", run.fleet);
    require_tag(&run, &tag).await;

    let beat = post(&http, &run, "/v1/runners/me/heartbeats", &capable_beat()).await;
    assert_eq!(beat.status().as_u16(), 200, "the runner proves its cage");
    assert_never_offered(
        &http,
        &run,
        "a runner without the tag was offered the fleet",
    )
    .await;

    edit_labels(&run, &[ENROLLED_LABEL, &tag]).await;
    let (lease_id, fence) = poll_for_seeded_lease(&http, &run).await;
    let report = report_body(&lease_id, &run.event_id, fence);
    let settled = post(&http, &run, "/v1/runners/me/reports", &report).await;
    assert_eq!(
        settled.status().as_u16(),
        200,
        "the labelled runner finishes the work"
    );

    edit_labels(&run, &[ENROLLED_LABEL]).await;
    let _next = run.enqueue_event(EVENT_TYPE).await;
    assert_never_offered(
        &http,
        &run,
        "a runner whose tag was edited away was offered the fleet again",
    )
    .await;

    supervisor.shutdown().await;
    run.cleanup().await;
}
