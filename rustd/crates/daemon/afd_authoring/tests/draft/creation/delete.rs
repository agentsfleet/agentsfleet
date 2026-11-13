//! A delete is one transaction: it ends setup and live work and removes the
//! fleet together, so a refused delete leaves every row as it was, and a
//! runner hears its Cancel only after the delete committed.

use super::super::mint;
use super::fixture::Case;
use super::grants::{cards, requiring_github};
use super::running_cancellation::{TestResult, assert_cancel, seed_lease};
use af_core::{error_code, id::Uuid7};
use afd_authoring::types::DraftState;
use afd_bus::Subject;

/// The draft a saved fleet points back to, and its state.
async fn draft_of(c: &Case, fleet: &Uuid7) -> Option<(Uuid7, String)> {
    sqlx::query_as("SELECT id, status FROM authoring.drafts WHERE fleet_id = $1")
        .bind(fleet)
        .fetch_optional(&mut *c.base.connection().await)
        .await
        .expect("draft read")
}

/// A lease's status and whether a cancel was asked of it.
async fn lease_state(c: &Case, lease: &Uuid7) -> Option<(String, bool)> {
    sqlx::query_as(
        "SELECT status, cancel_requested_at IS NOT NULL FROM runner.leases WHERE id = $1",
    )
    .bind(lease)
    .fetch_optional(&mut *c.base.connection().await)
    .await
    .expect("lease read")
}

async fn fleet_rows(c: &Case, fleet: &Uuid7) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM fleet.fleets WHERE id = $1")
        .bind(fleet)
        .fetch_one(&mut *c.base.connection().await)
        .await
        .expect("fleet count")
}

#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_stale_delete_changes_nothing() -> TestResult {
    let c = Case::create().await;
    c.fund().await;
    let saved = c.save(&mint(), requiring_github(&c).await).await;
    assert_eq!(saved.status, "installing");
    let lease = seed_lease(&c, &saved.id).await?;
    let before = (
        cards(&c, &saved.id).await,
        lease_state(&c, &lease).await,
        draft_of(&c, &saved.id).await,
        c.creations.read(c.base.access(), &saved.id).await?.status,
    );
    assert_eq!(before.0.len(), 1, "one open setup card");
    assert!(before.2.is_some(), "the draft names its fleet");

    let refused = c
        .creations
        .delete(c.base.access(), &saved.id, "\"stale\"")
        .await
        .expect_err("a stale tag deletes nothing");
    assert_eq!(refused.code(), error_code::AUTHORING_STALE);

    let after = (
        cards(&c, &saved.id).await,
        lease_state(&c, &lease).await,
        draft_of(&c, &saved.id).await,
        c.creations.read(c.base.access(), &saved.id).await?.status,
    );
    assert_eq!(
        after, before,
        "cards, lease, draft link and fleet are untouched"
    );
    assert_eq!(fleet_rows(&c, &saved.id).await, 1);
    c.cleanup().await;
    Ok(())
}

#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_delete_ends_setup_and_work_then_removes_the_fleet() -> TestResult {
    let c = Case::create().await;
    c.fund().await;
    let saved = c.save(&mint(), requiring_github(&c).await).await;
    assert_eq!(cards(&c, &saved.id).await.len(), 1);
    let lease = seed_lease(&c, &saved.id).await?;
    let (draft, _) = draft_of(&c, &saved.id).await.expect("linked draft");
    let address = Subject::RunnerCommand(c.base.runner.clone());
    let mut commands = c.bus.subscribe(&address).await?;

    c.creations
        .delete(c.base.access(), &saved.id, &saved.etag)
        .await?;

    assert_eq!(fleet_rows(&c, &saved.id).await, 0, "the fleet is gone");
    assert!(
        cards(&c, &saved.id).await.is_empty(),
        "its cards are closed"
    );
    assert_eq!(
        lease_state(&c, &lease).await,
        None,
        "its live lease is gone"
    );
    let restored: (Option<Uuid7>, String) =
        sqlx::query_as("SELECT fleet_id, status FROM authoring.drafts WHERE id = $1")
            .bind(&draft)
            .fetch_one(&mut *c.base.connection().await)
            .await?;
    assert_eq!(restored, (None, DraftState::Editing.as_str().to_owned()));
    assert_cancel(&address, &lease, &mut commands).await?;
    drop(commands);
    c.cleanup().await;
    Ok(())
}
