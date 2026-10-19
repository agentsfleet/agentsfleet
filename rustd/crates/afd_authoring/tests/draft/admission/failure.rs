//! Pre-claim refusal survives reload with a bounded action and no obligation.

use super::{claim, fixture, funded, queue};
use afd_authoring::types::MessageState;
use afd_core::error_code;

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn revoked_model_leaves_a_durable_recovery_action() {
    let f = funded(1).await;
    let draft = queue(&f, "Selected model later disappears").await;
    sqlx::query("DELETE FROM model.tenant_models WHERE id=$1::uuid")
        .bind(f.model.as_str())
        .execute(&mut *f.connection().await)
        .await
        .expect("disable owned entry");
    assert!(claim(&f, &draft).await.is_none());
    let failed = fixture::failure(&f, &draft).await;
    assert_eq!(failed.state, MessageState::Failed);
    let failure = failed.failure.expect("reload bounded failure");
    assert_eq!(failure.error_code, error_code::AUTHORING_MODEL);
    assert_eq!(failure.recovery_action, "choose_authoring_model");
    assert_eq!(fixture::attempts(&f).await, 0);
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn missing_wallet_is_a_durable_billing_failure_without_an_attempt() {
    let f = funded(1).await;
    let draft = queue(&f, "Owner billing is required").await;
    sqlx::query("DELETE FROM billing.wallets WHERE tenant_id=$1::uuid")
        .bind(f.tenant.as_str())
        .execute(&mut *f.connection().await)
        .await
        .expect("remove owned wallet");
    assert!(claim(&f, &draft).await.is_none());
    let failed = fixture::failure(&f, &draft).await;
    let failure = failed.failure.expect("reload bounded failure");
    assert_eq!(failure.error_code, error_code::AUTHORING_BALANCE);
    assert_eq!(failure.recovery_action, "owner_billing");
    assert_eq!(fixture::attempts(&f).await, 0);
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn membership_revocation_cancels_queued_work_before_any_placement() {
    let f = funded(1).await;
    let created = f
        .drafts
        .create(
            f.member_access(),
            f.request(super::mint(), Some("Private member question")),
            super::now(),
        )
        .await
        .expect("member's private draft");
    sqlx::query("DELETE FROM identity.memberships WHERE tenant_id=$1::uuid AND user_id=$2::uuid")
        .bind(f.tenant.as_str())
        .bind(f.member.as_str())
        .execute(&mut *f.connection().await)
        .await
        .expect("committed membership revocation");
    assert!(claim(&f, &created.draft).await.is_none());
    let state: (String, Option<String>) = sqlx::query_as(
        "SELECT status,failure_reason FROM authoring.messages WHERE draft_id=$1::uuid",
    )
    .bind(created.draft.id.as_str())
    .fetch_one(&mut *f.connection().await)
    .await
    .expect("owned private state");
    assert_eq!(state, ("cancelled".into(), None));
    assert_eq!(fixture::attempts(&f).await, 0);
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn oversized_native_context_fails_before_capacity_or_provider_obligation() {
    let f = funded(1).await;
    let draft = queue(&f, &"語".repeat(10_000)).await;
    sqlx::query("UPDATE model.platform_models SET context_cap_tokens=8192 WHERE id=$1::uuid")
        .bind(f.catalogue[0].as_str())
        .execute(&mut *f.connection().await)
        .await
        .expect("controlled context ceiling");
    assert!(claim(&f, &draft).await.is_none());
    let failed = fixture::failure(&f, &draft).await;
    assert_eq!(
        failed.failure.expect("bounded context failure").error_code,
        error_code::AUTHORING_TOKEN
    );
    assert_eq!(fixture::attempts(&f).await, 0);
    f.cleanup().await;
}
