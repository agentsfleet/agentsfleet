//! Observational data is excluded; effective selections and sealed versions are included.

use super::*;
use afd_authoring::types::{CheckKind, CheckState};
use afd_core::error_code::AUTHORING_REVIEW;

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn timestamp_wallet_and_authoring_selection_do_not_change_reviewed_meaning() {
    let f = Fixture::create().await;
    let draft = draft(&f, false).await;
    let first = review(&f, &draft, false).await;
    assert!(
        first
            .readiness
            .iter()
            .any(|check| check.kind == CheckKind::Model && check.state == CheckState::Ready)
    );
    assert!(
        first
            .readiness
            .iter()
            .any(|check| check.kind == CheckKind::Wallet && check.state == CheckState::Missing)
    );
    sqlx::query("INSERT INTO billing.wallets(tenant_id,balance_nanos,created_at,updated_at) VALUES($1::uuid,$2,now(),now())")
        .bind(f.tenant.as_str()).bind(crate::draft::FUNDED_NANOS).execute(&mut *f.connection().await).await.expect("fund owner wallet");
    let patch: DraftPatch = serde_json::from_value(
        json!({"authoring_model_ref":f.other_model,"title":"Display title"}),
    )
    .expect("metadata edit");
    f.drafts
        .patch(f.access(), &draft.id, Some(&draft.etag), patch, now())
        .await
        .expect("idle metadata edit");
    let second = review(&f, &draft, false).await;
    assert_eq!(first.review_digest, second.review_digest);
    assert!(
        second
            .readiness
            .iter()
            .any(|check| check.kind == CheckKind::Wallet && check.state == CheckState::Ready)
    );
    assert!(second.observed_at >= first.observed_at);
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn independent_instance_selection_invalidates_review_without_a_candidate_revision() {
    let f = Fixture::create().await;
    let draft = draft(&f, false).await;
    let first = review(&f, &draft, false).await;
    let patch: DraftPatch =
        serde_json::from_value(json!({"instance_name":"second-instance"})).expect("name selection");
    let changed = f
        .drafts
        .patch(f.access(), &draft.id, Some(&draft.etag), patch, now())
        .await
        .expect("select instance name");
    assert_eq!(
        changed.candidate.as_ref().expect("candidate").id,
        draft.candidate.as_ref().expect("candidate").id
    );
    let second = review(&f, &draft, false).await;
    assert_eq!(second.effective.instance_name, "second-instance");
    let error = second
        .require_digest(&first.review_digest)
        .expect_err("stale human review");
    assert_eq!(error.code(), AUTHORING_REVIEW);
    assert_eq!(error.current_state(), Some("review_required"));
    assert_eq!(error.review_digest(), Some(second.review_digest.as_str()));
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn model_generation_changes_digest_and_deleted_selection_never_falls_back() {
    let f = Fixture::create().await;
    let draft = draft(&f, false).await;
    let first = review(&f, &draft, false).await;
    sqlx::query(
        "UPDATE model.platform_models SET context_cap_tokens=context_cap_tokens-1 WHERE id=$1::uuid",
    )
    .bind(f.catalogue[0].as_str())
    .execute(&mut *f.connection().await)
    .await
    .expect("model generation drift");
    let changed = review(&f, &draft, false).await;
    assert_ne!(first.review_digest, changed.review_digest);
    sqlx::query("DELETE FROM model.tenant_models WHERE id=$1::uuid")
        .bind(f.model.as_str())
        .execute(&mut *f.connection().await)
        .await
        .expect("disable selection");
    let missing = review(&f, &draft, false).await;
    assert_eq!(
        missing.effective.runtime_model_ref,
        Some(f.model.to_string())
    );
    assert!(
        missing
            .readiness
            .iter()
            .any(|check| check.kind == CheckKind::Model && check.state == CheckState::Missing)
    );
    assert_ne!(changed.review_digest, missing.review_digest);
    f.cleanup().await;
}
