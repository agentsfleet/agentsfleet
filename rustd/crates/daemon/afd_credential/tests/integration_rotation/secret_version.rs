//! `vault.secrets.resource_version` moves on every update, whichever writer
//! runs, so a cache keyed on it never serves a replaced key (M245 §2).

use std::sync::Arc;

use af_crypto::entropy::Entropy;
use afd_vault::{SECRET_FIRST_VERSION, SecretBody, SecretName};
use serde_json::value::RawValue;

use super::{Fixture, KEY_NAME};

/// The stored row's version and KEK version.
async fn versions(fixture: &Fixture) -> (i64, i32) {
    let mut connection = fixture.database.acquire().await.expect("an API connection");
    sqlx::query_as(
        "SELECT resource_version, kek_version FROM vault.secrets \
          WHERE workspace_id = $1 AND key_name = $2",
    )
    .bind(&fixture.workspace)
    .bind(KEY_NAME)
    .fetch_one(&mut *connection)
    .await
    .expect("the seeded secret is readable")
}

// Value: protects=a same-KEK replace and a re-seal each move the version;
// fails_when=the trigger is missing or a writer bypasses it;
// why_new=the cache key needs a column no writer can skip; seam=none;
// no_claim=concurrent writers.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn secret_version_moves_on_same_kek_replace() {
    let fixture = Fixture::create().await;
    fixture
        .seed(KEY_NAME, br#"{"refresh_token":"old","api_key":"a"}"#)
        .await;
    let (first, kek) = versions(&fixture).await;
    assert_eq!(first, SECRET_FIRST_VERSION);

    // The workspace replace: `update_secret.sql`, sealed under the same KEK.
    let admin = afd_vault::Vault::new(
        fixture.database.clone(),
        Arc::clone(&fixture.kek),
        Entropy::new(),
    );
    let name = SecretName::parse(KEY_NAME).expect("the fixture name is within bounds");
    let raw = RawValue::from_string(r#"{"refresh_token":"old","api_key":"b"}"#.to_owned())
        .expect("the fixture body is JSON");
    let body = SecretBody::parse(&raw).expect("the fixture body is a non-empty object");
    admin
        .replace(&fixture.workspace, &name, &body)
        .await
        .expect("the held name is replaced");
    assert_eq!(versions(&fixture).await, (SECRET_FIRST_VERSION + 1, kek));

    // The re-seal: `update_secret_envelope.sql`, which also writes `kek_version`.
    fixture
        .vault
        .rotate_refresh_token(fixture.key(KEY_NAME), "old", "new")
        .await
        .expect("the matching handle rotates");
    assert_eq!(versions(&fixture).await.0, SECRET_FIRST_VERSION + 2);

    fixture.cleanup().await;
}
