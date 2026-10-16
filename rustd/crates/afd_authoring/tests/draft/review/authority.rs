//! Joined creators use owner resources; current revocation still wins before review.

use super::*;
use afd_authoring::types::{CheckKind, CheckState};
use afd_core::error_code::AUTHORING_NOT_FOUND;

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn joined_creator_reviews_owner_model_and_loses_access_after_revocation() {
    let f = Fixture::create().await;
    let draft = draft(&f, true).await;
    let reviewed = review(&f, &draft, true).await;
    assert_eq!(
        reviewed.effective.runtime_model_ref,
        Some(f.model.to_string())
    );
    assert!(
        reviewed
            .readiness
            .iter()
            .any(|check| check.kind == CheckKind::Model && check.state == CheckState::Ready)
    );
    let foreign = setup(&f)
        .read_draft(f.access(), &draft.id)
        .await
        .expect_err("owner cannot read member's private draft");
    assert_eq!(foreign.code(), AUTHORING_NOT_FOUND);
    sqlx::query("DELETE FROM core.memberships WHERE tenant_id=$1::uuid AND user_id=$2::uuid")
        .bind(f.tenant.as_str())
        .bind(f.member.as_str())
        .execute(&mut *f.connection().await)
        .await
        .expect("revoke joined member");
    let revoked = setup(&f)
        .read_draft(f.member_access(), &draft.id)
        .await
        .expect_err("current membership required");
    assert_eq!(revoked.code(), AUTHORING_NOT_FOUND);
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn requirements_stay_declared_while_missing_credential_readiness_is_visible() {
    let f = Fixture::create().await;
    store_credential(&f).await;
    let draft = draft(&f, false).await;
    let patch: DraftPatch = serde_json::from_value(json!({"config_ops":[{"op":"replace","path":"/config/x-agentsfleet/credentials","value":["deploy_key","not_configured"]}]})).expect("credential requirements");
    f.drafts
        .patch(f.access(), &draft.id, Some(&draft.etag), patch, now())
        .await
        .expect("valid declaration");
    let reviewed = review(&f, &draft, false).await;
    assert_eq!(
        reviewed.requirements.credentials,
        ["deploy_key", "not_configured"]
    );
    assert!(
        reviewed
            .readiness
            .iter()
            .any(|check| check.kind == CheckKind::Credential
                && check.subject == "not_configured"
                && check.state == CheckState::Missing)
    );
    assert!(
        reviewed
            .readiness
            .iter()
            .any(|check| check.kind == CheckKind::Credential
                && check.subject == "deploy_key"
                && check.state == CheckState::Ready)
    );
    let json = serde_json::to_string(&reviewed).expect("review JSON");
    assert!(!json.contains("fixture-authoring-key"));
    assert!(!json.contains("api_key"));
    f.cleanup().await;
}

async fn store_credential(f: &Fixture) {
    use afd_crypto::entropy::Entropy;
    use afd_vault::{SecretBody, SecretName, Vault};
    let key = Kek::from_hex("00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff")
        .expect("fixture key");
    let raw = serde_json::value::RawValue::from_string(
        r#"{"api_key":"fixture-authoring-key"}"#.to_owned(),
    )
    .expect("secret object");
    let body = SecretBody::parse(&raw).expect("secret body");
    Vault::new(f.db.clone(), Arc::new(key), Entropy::new())
        .create(
            &f.workspace,
            &SecretName::parse("deploy_key").expect("name"),
            &body,
            now(),
        )
        .await
        .expect("seal declared credential");
}
