//! Platform-admin custody, rotation and refusal through the production resolver.

use std::{error::Error as _, sync::Arc};

use afd_core::{error_code, json::Unreadable};
use afd_credential::provider::Providers;
use afd_crypto::entropy::Entropy;
use afd_db::{config::DbRole, test_util::TestDatabase};
use afd_vault::{Deleted, SecretBody, SecretName, Vault};
use afd_wire::decision::DecisionApi;
use serde_json::{Value, json, value::RawValue};

use super::{Fixture, NOW};

fn reader(fixture: &Fixture) -> Providers {
    Providers::new(
        fixture.database.clone(),
        Arc::clone(&fixture.kek),
        Entropy::new(),
    )
}

fn writer(fixture: &Fixture) -> Vault {
    Vault::new(
        fixture.database.clone(),
        Arc::clone(&fixture.kek),
        Entropy::new(),
    )
}

fn body(value: &Value) -> SecretBody {
    let raw = RawValue::from_string(value.to_string()).expect("synthetic secret JSON");
    SecretBody::parse(&raw).expect("bounded secret object")
}

async fn assert_key(reader: &Providers, fixture: &Fixture, api: DecisionApi, expected: &str) {
    let held = reader
        .resolve_decision_key(&fixture.workspace, api)
        .await
        .expect("admin key resolves");
    assert_eq!(held.expose(), expected);
    assert_eq!(format!("{held:?}"), "SecretString(redacted)");
}

async fn assert_missing(reader: &Providers, fixture: &Fixture, api: DecisionApi) {
    let refused = reader
        .resolve_decision_key(&fixture.workspace, api)
        .await
        .expect_err("no usable admin key");
    assert_eq!(refused.code(), error_code::INTERNAL_OPERATION_FAILED);
    assert!(refused.is_config_permanent());
    assert!(!refused.is_datastore_unavailable());
    assert!(refused.source().is_none());
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn decision_keys_follow_admin_rotation_and_never_fall_back_to_tenant_keys() {
    let admin = Fixture::create().await;
    let tenant = Fixture::create().await;
    tenant
        .seed("typesafe", br#"{"api_key":"tenant-only-value"}"#)
        .await;
    let reader = reader(&admin);
    assert_missing(&reader, &admin, DecisionApi::Typesafe).await;
    let writer = writer(&admin);
    let name = SecretName::parse("typesafe").expect("fixed bag name");
    writer
        .create(
            &admin.workspace,
            &name,
            &body(&json!({"api_key":"first-admin-value"})),
            NOW,
        )
        .await
        .expect("admin stores key");
    assert_key(&reader, &admin, DecisionApi::Typesafe, "first-admin-value").await;
    writer
        .replace(
            &admin.workspace,
            &name,
            &body(&json!({"api_key":"rotated-admin-value"})),
        )
        .await
        .expect("admin rotates key");
    assert_key(
        &reader,
        &admin,
        DecisionApi::Typesafe,
        "rotated-admin-value",
    )
    .await;
    assert_eq!(
        writer
            .directory()
            .delete(&admin.workspace, &name)
            .await
            .expect("admin deletes key"),
        Deleted::Removed
    );
    assert_missing(&reader, &admin, DecisionApi::Typesafe).await;
    tenant.cleanup().await;
    admin.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn decision_apis_resolve_distinct_fixed_bags_and_refuse_unusable_bodies() {
    let admin = Fixture::create().await;
    let reader = reader(&admin);
    let writer = writer(&admin);
    for (api, bag) in [
        (DecisionApi::Typesafe, "typesafe"),
        (DecisionApi::Openai, "openai"),
        (DecisionApi::Perplexity, "perplexity"),
        (DecisionApi::Cloudflare, "cloudflare"),
    ] {
        let name = SecretName::parse(bag).expect("fixed bag name");
        writer
            .create(
                &admin.workspace,
                &name,
                &body(&json!({"api_key":bag,"unrelated":"kept in vault"})),
                NOW,
            )
            .await
            .expect("independent admin key");
        assert_key(&reader, &admin, api, bag).await;
    }
    let name = SecretName::parse("typesafe").expect("fixed bag name");
    for invalid in [
        json!({"api_key":42}),
        json!({"api_key":{"private":"must-not-appear"}}),
        json!({"other":"must-not-appear"}),
    ] {
        writer
            .replace(&admin.workspace, &name, &body(&invalid))
            .await
            .expect("a secret object may hold other shapes");
        let refused = reader
            .resolve_decision_key(&admin.workspace, DecisionApi::Typesafe)
            .await
            .expect_err("unusable credential");
        assert_eq!(refused.code(), error_code::INTERNAL_OPERATION_FAILED);
        assert!(refused.is_config_permanent());
        let cause = refused.source().expect("redacted parsing cause");
        assert!(cause.downcast_ref::<Unreadable>().is_some());
        assert!(!format!("{refused:?} {cause}").contains("must-not-appear"));
    }
    writer
        .replace(&admin.workspace, &name, &body(&json!({"api_key":""})))
        .await
        .expect("empty secret field");
    assert_missing(&reader, &admin, DecisionApi::Typesafe).await;
    assert_key(&reader, &admin, DecisionApi::Openai, "openai").await;
    admin.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn unreadable_envelopes_and_closed_storage_preserve_failures_without_another_key() {
    let admin = Fixture::create().await;
    admin
        .seed("typesafe", br#"{"api_key":"never-return-this"}"#)
        .await;
    sqlx::query("UPDATE vault.secrets SET secret_ciphertext=set_byte(secret_ciphertext,0,get_byte(secret_ciphertext,0)#1) WHERE workspace_id=$1::uuid AND key_name=$2")
        .bind(admin.workspace.as_str()).bind("typesafe").execute(&mut *admin.database.acquire().await.expect("corruption fixture connection")).await.expect("simulate damaged envelope");
    let refused = reader(&admin)
        .resolve_decision_key(&admin.workspace, DecisionApi::Typesafe)
        .await
        .expect_err("no fallback after failed authentication");
    assert_eq!(refused.code(), error_code::INTERNAL_OPERATION_FAILED);
    assert!(!refused.is_config_permanent());
    assert!(refused.source().is_some());
    assert!(!format!("{refused:?}").contains("never-return-this"));

    let lane = TestDatabase::shared();
    let database = lane.open(DbRole::Api, &[]).await;
    let unavailable = Providers::new(database.clone(), Arc::clone(&admin.kek), Entropy::new());
    database.close().await;
    let refused = unavailable
        .resolve_decision_key(&admin.workspace, DecisionApi::Typesafe)
        .await
        .expect_err("closed storage must remain unavailable");
    assert_eq!(refused.code(), error_code::INTERNAL_DB_UNAVAILABLE);
    assert!(refused.is_datastore_unavailable());
    assert!(!refused.is_config_permanent());
    assert!(refused.source().is_some());
    lane.cleanup().await;
    admin.cleanup().await;
}
