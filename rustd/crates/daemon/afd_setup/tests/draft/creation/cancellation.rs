//! Archive wins against later retries and ends the draft it was saved from.

use super::super::{fixture::now, mint};
use super::fixture::{Case, NO_TURNS};
use afd_setup::types::{DraftState, Start};

#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn archive_closes_the_draft_and_replay_cannot_resurrect_the_fleet() {
    let c = Case::create().await;
    let (draft, request) = c.request("cancel-setup", Start::None).await;
    let key = mint();
    let saved = c.save(&key, request.clone()).await;
    assert_eq!(saved.status, "installing");
    let archived = c
        .creations
        .patch(
            c.base.access(),
            &saved.id,
            Some(&saved.etag),
            Some("archived"),
            None,
            now(),
        )
        .await
        .expect("atomic setup cancellation");
    assert_eq!(archived.status, "archived");
    assert_eq!(archived.setup.state, "cancelled");
    let closed = c
        .base
        .drafts
        .read(c.base.access(), &draft)
        .await
        .expect("closed draft stays readable");
    assert_eq!(closed.state, DraftState::Closed);
    c.fund().await;
    assert_eq!(c.save(&key, request.clone()).await.status, "archived");
    assert_eq!(c.turns(&saved.id).await, NO_TURNS);
    c.creations
        .save(c.base.access(), &mint(), request, now())
        .await
        .expect_err("a closed draft saves nothing");
    assert_eq!(c.fleet_count().await, 1, "no second fleet");
    assert_eq!(
        c.creations
            .read(c.base.access(), &saved.id)
            .await
            .expect("retained")
            .status,
        "archived"
    );
    c.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn stale_archive_does_not_cancel_a_newer_resource_revision() {
    let c = Case::create().await;
    let (_, request) = c.request("stale-archive", Start::None).await;
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
            Some("archived"),
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
