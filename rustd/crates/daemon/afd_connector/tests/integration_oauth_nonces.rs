//! A connect nonce in Postgres spends once, and an expired one never.
//!
//! Marked `#[ignore]` so the unit lane compiles and lints this without
//! datastores; `make test-integration-rustd` is the lane that runs it.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::sync::Arc;

use af_crypto::entropy::Entropy;
use afd_connector::provider::Provider;
use afd_connector::state::nonce;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};
use jiff::Timestamp;
use tokio::sync::Barrier;

const SPENDERS: usize = 100;

async fn rows(database: &afd_db::Db, provider: Provider) -> i64 {
    let mut connection = database.acquire().await.expect("a connection");
    sqlx::query_scalar("SELECT count(*) FROM connector.oauth_nonces WHERE provider = $1")
        .bind(provider.id())
        .fetch_one(&mut *connection)
        .await
        .expect("the count reads")
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn nonce_spends_once_under_contention() {
    let lane = TestDatabase::shared();
    let database = lane.open(DbRole::Api, &[]).await;
    let entropy = Entropy::new();
    let fresh = nonce::mint(&entropy).expect("the host has entropy");
    nonce::remember(&database, &entropy, Provider::Jira, &fresh, Timestamp::now())
        .await
        .expect("the nonce is remembered");

    let barrier = Arc::new(Barrier::new(SPENDERS));
    let mut spenders = Vec::new();
    for _ in 0..SPENDERS {
        let (database, barrier, fresh) = (database.clone(), Arc::clone(&barrier), fresh.clone());
        spenders.push(tokio::spawn(async move {
            barrier.wait().await;
            nonce::consume(&database, Provider::Jira, &fresh).await
        }));
    }
    let mut won = 0;
    for spender in spenders {
        if spender.await.expect("a spender ran").expect("the store answered") {
            won += 1;
        }
    }

    assert_eq!(won, 1, "exactly one callback spends the nonce");
    assert!(
        !nonce::consume(&database, Provider::Jira, &fresh)
            .await
            .expect("the store answered"),
        "a spent nonce stays spent"
    );
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn one_providers_nonce_never_spends_under_another() {
    let lane = TestDatabase::shared();
    let database = lane.open(DbRole::Api, &[]).await;
    let entropy = Entropy::new();
    let fresh = nonce::mint(&entropy).expect("the host has entropy");
    nonce::remember(&database, &entropy, Provider::Slack, &fresh, Timestamp::now())
        .await
        .expect("the nonce is remembered");

    let crossed = nonce::consume(&database, Provider::Linear, &fresh)
        .await
        .expect("the store answered");

    assert!(!crossed, "a Linear callback cannot spend a Slack connect's nonce");
    assert!(
        nonce::consume(&database, Provider::Slack, &fresh)
            .await
            .expect("the store answered"),
        "the right provider still spends it"
    );
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn expired_nonce_is_refused() {
    let lane = TestDatabase::shared();
    let database = lane.open(DbRole::Api, &[]).await;
    let entropy = Entropy::new();
    let stale = nonce::mint(&entropy).expect("the host has entropy");
    // The table refuses UPDATE, so the expired row is written expired.
    let mut connection = database.acquire().await.expect("a connection");
    sqlx::query(
        "INSERT INTO connector.oauth_nonces (id, provider, nonce_digest, created_at, expires_at) \
         VALUES ($1, $2, sha256(convert_to($3, 'UTF8')), now(), clock_timestamp() - interval '1 second')",
    )
    .bind(mint_id())
    .bind(Provider::Zoho.id())
    .bind(&stale)
    .execute(&mut *connection)
    .await
    .expect("the expired row is written");
    drop(connection);
    let before = rows(&database, Provider::Zoho).await;

    let spent = nonce::consume(&database, Provider::Zoho, &stale)
        .await
        .expect("the store answered");

    assert!(!spent, "an expired nonce answers nobody");
    assert_eq!(
        rows(&database, Provider::Zoho).await,
        before,
        "a refused spend deletes nothing; the sweep owns expired rows"
    );
}
