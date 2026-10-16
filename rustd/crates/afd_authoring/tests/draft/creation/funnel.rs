//! Setup reports its finish once, when activation commits, and its
//! cancellation once, when a Kill lands on a fleet still installing.

use super::super::{fixture::now, funnel::property, mint};
use super::fixture::Case;
use afd_authoring::{SavedFleet, types::Start};
use afd_observability::Telemetry;

const FINISHED: &str = "fleet_setup_finished";
const CANCELLED: &str = "fleet_setup_cancelled";
const KILLED: &str = "killed";

/// Asserts `event` is the owner's step about `saved`.
fn about(c: &Case, event: &Telemetry, saved: &SavedFleet) {
    assert_eq!(event.actor(), Some(c.base.owner.as_str()));
    let workspace = property(event, "workspace_id");
    assert_eq!(workspace.as_deref(), Some(c.base.workspace.as_str()));
    assert_eq!(
        property(event, "fleet_id").as_deref(),
        Some(saved.id.as_str())
    );
}

async fn kill(c: &Case, fleet: &SavedFleet) -> afd_authoring::Result<SavedFleet> {
    let access = c.base.access();
    let tag = Some(fleet.etag.as_str());
    c.creations
        .patch(access, &fleet.id, tag, Some(KILLED), None, now())
        .await
}

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn should_report_setup_finished_once_when_activation_commits() {
    let c = Case::create().await;
    let (_, request) = c.request("funnel-finish", Start::None).await;
    let key = mint();
    let waiting = c.save(&key, request.clone()).await;
    assert_eq!(waiting.status, "installing");
    assert!(
        c.base.reported(FINISHED).is_empty(),
        "no wallet, no activation"
    );
    c.fund().await;
    let active = c.save(&key, request.clone()).await;
    assert_eq!(active.status, "active");
    assert_eq!(c.save(&key, request).await.status, "active", "exact replay");
    let finished = c.base.reported(FINISHED);
    assert_eq!(
        finished.len(),
        1,
        "one activation, however often Save retries"
    );
    about(&c, &finished[0], &active);
    kill(&c, &active).await.expect("a live fleet can be killed");
    assert!(
        c.base.reported(CANCELLED).is_empty(),
        "killing a live fleet is not a setup cancellation"
    );
    c.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn should_report_setup_cancelled_once_and_not_for_a_refused_kill() {
    let c = Case::create().await;
    let (_, request) = c.request("funnel-cancel", Start::None).await;
    let saved = c.save(&mint(), request).await;
    assert_eq!(saved.status, "installing");
    let killed = kill(&c, &saved).await.expect("setup cancellation");
    assert_eq!(killed.status, KILLED);
    kill(&c, &saved).await.expect_err("a stale tag is refused");
    kill(&c, &killed)
        .await
        .expect_err("a killed fleet cannot be killed again");
    let cancelled = c.base.reported(CANCELLED);
    assert_eq!(cancelled.len(), 1, "one cancellation, refusals add nothing");
    about(&c, &cancelled[0], &saved);
    assert_eq!(c.base.reported(FINISHED).len(), 0, "setup never finished");
    c.cleanup().await;
}
