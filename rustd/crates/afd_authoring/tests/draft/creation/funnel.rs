//! Setup reports its finish once, when activation commits, and its
//! cancellation once, when a Kill lands on a fleet still installing.

use super::super::{fixture::now, funnel::property, mint};
use super::fixture::Case;
use afd_authoring::{SavedFleet, types::Start};
use afd_core::error_code;
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
    for status in [Some(KILLED), Some("active"), None] {
        let refused = c
            .creations
            .patch(
                c.base.access(),
                &killed.id,
                Some(&killed.etag),
                status,
                Some("renamed"),
                now(),
            )
            .await
            .expect_err("a killed fleet is a tombstone for every edit");
        assert_eq!(refused.code(), error_code::AGENTSFLEET_NOT_FOUND);
    }
    let unchanged = c
        .creations
        .read(c.base.access(), &killed.id)
        .await
        .expect("tombstone remains readable");
    assert_eq!(unchanged.status, KILLED);
    assert_eq!(unchanged.name, killed.name);
    assert_eq!(unchanged.etag, killed.etag);
    let cancelled = c.base.reported(CANCELLED);
    assert_eq!(cancelled.len(), 1, "one cancellation, refusals add nothing");
    about(&c, &cancelled[0], &saved);
    assert_eq!(c.base.reported(FINISHED).len(), 0, "setup never finished");
    c.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn should_leave_installing_setup_unchanged_after_empty_or_invalid_edits() {
    let c = Case::create().await;
    let (_, request) = c.request("funnel-refused-edit", Start::None).await;
    let saved = c.save(&mint(), request).await;
    let refusals = [
        (
            None,
            error_code::INVALID_REQUEST,
            "the setup request is invalid",
        ),
        (
            Some("stopped"),
            error_code::AUTHORING_STATE,
            "transition_refused",
        ),
        (
            Some("unsupported"),
            error_code::AUTHORING_STATE,
            "transition_refused",
        ),
    ];
    for (status, code, message) in refusals {
        assert_refused_edit(&c, &saved, status, code, message).await;
    }
    assert_eq!(c.base.reported(CANCELLED), [] as [Telemetry; 0]);
    assert_eq!(c.base.reported(FINISHED), [] as [Telemetry; 0]);
    c.fund().await;
    let active = c
        .creations
        .patch(
            c.base.access(),
            &saved.id,
            Some(&saved.etag),
            Some("active"),
            None,
            now(),
        )
        .await
        .expect("the unchanged version can still resume setup");
    assert_eq!(active.status, "active");
    assert_eq!(c.base.reported(FINISHED).len(), 1);
    c.cleanup().await;
}

async fn assert_refused_edit(
    c: &Case,
    saved: &SavedFleet,
    status: Option<&str>,
    code: error_code::ErrorCode,
    message: &str,
) {
    let refused = c
        .creations
        .patch(
            c.base.access(),
            &saved.id,
            Some(&saved.etag),
            status,
            None,
            now(),
        )
        .await
        .expect_err("an empty edit or refused transition cannot change setup");
    assert_eq!(refused.code(), code);
    assert!(refused.to_string().contains(message), "{refused}");
    let unchanged = c
        .creations
        .read(c.base.access(), &saved.id)
        .await
        .expect("setup remains readable");
    assert_eq!(unchanged.status, saved.status);
    assert_eq!(unchanged.name, saved.name);
    assert_eq!(unchanged.etag, saved.etag);
}
