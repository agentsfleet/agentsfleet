//! Kill wins against later retries and returns an unfinished draft to its creator.

use super::super::{fixture::now, mint};
use super::fixture::{Case, NO_ADMISSIONS};
use afd_authoring::types::{DraftState, SaveFleet, Start};
use afd_core::id::Uuid7;

/// `request` under a new instance name: the draft renamed through a patch on
/// its current tag, and the review digest re-read for the renamed source.
async fn renamed(c: &Case, draft: &Uuid7, request: SaveFleet, name: &str) -> SaveFleet {
    let mut another = request;
    another.name = name.to_owned();
    let fresh = c
        .setup
        .read_draft(c.base.access(), draft)
        .await
        .expect("current source");
    let patch = serde_json::from_value(serde_json::json!({"instance_name":another.name}))
        .expect("name patch");
    c.base
        .drafts
        .patch(c.base.access(), draft, Some(&fresh.etag), patch, now())
        .await
        .expect("new reviewed instance name");
    another.review_digest = c
        .setup
        .read_draft(c.base.access(), draft)
        .await
        .expect("new review")
        .review
        .expect("review")
        .review_digest;
    another
}

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn kill_restores_private_editing_and_replay_cannot_resurrect_the_fleet() {
    let c = Case::create().await;
    let (draft, request) = c.request("cancel-setup", Start::None).await;
    let key = mint();
    let saved = c.save(&key, request.clone()).await;
    assert_eq!(saved.status, "installing");
    let killed = c
        .creations
        .patch(
            c.base.access(),
            &saved.id,
            Some(&saved.etag),
            Some("killed"),
            None,
            now(),
        )
        .await
        .expect("atomic setup cancellation");
    assert_eq!(killed.status, "killed");
    assert_eq!(killed.setup.state, "cancelled");
    let restored = c
        .base
        .drafts
        .read(c.base.access(), &draft)
        .await
        .expect("private editing restored");
    assert_eq!(restored.state, DraftState::Editing);
    assert!(restored.linked_fleet_id.is_none());
    c.fund().await;
    assert_eq!(c.save(&key, request.clone()).await.status, "killed");
    assert_eq!(c.admissions(&saved.id).await, NO_ADMISSIONS);
    let another = renamed(&c, &draft, request, "new-setup").await;
    let saved_again = c.save(&mint(), another).await;
    assert_ne!(saved_again.id, saved.id);
    assert_eq!(saved_again.status, "active");
    c.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn stale_kill_does_not_cancel_a_newer_resource_revision() {
    let c = Case::create().await;
    let (_, request) = c.request("stale-kill", Start::None).await;
    let saved = c.save(&mint(), request).await;
    let renamed = c
        .creations
        .patch(
            c.base.access(),
            &saved.id,
            Some(&saved.etag),
            None,
            Some("renamed-setup"),
            now(),
        )
        .await
        .expect("new resource version");
    let error = c
        .creations
        .patch(
            c.base.access(),
            &saved.id,
            Some(&saved.etag),
            Some("killed"),
            None,
            now(),
        )
        .await
        .expect_err("stale cancellation refused");
    assert_eq!(error.etag(), Some(renamed.etag.as_str()));
    assert_eq!(
        c.creations
            .read(c.base.access(), &saved.id)
            .await
            .expect("retained")
            .status,
        "installing"
    );
    c.cleanup().await;
}
