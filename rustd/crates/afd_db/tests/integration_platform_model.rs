//! Shared model seeds use the registered schema and atomic identity/price writes.
#![cfg(feature = "test-util")]

use afd_db::{
    Result,
    config::DbRole,
    test_util::{ModelSeed, TestDatabase, mint_id},
};
use jiff::Timestamp;
use std::error::Error as _;

const PROVIDER: &str = "catalogue-fixture";
const VERIFIED: Timestamp = Timestamp::constant(1_760_000_000, 0);
const READ: &str = include_str!("integration_platform_model/read.sql");
const DELETE: &str = include_str!("integration_platform_model/delete.sql");

#[derive(Debug, sqlx::FromRow)]
struct Row {
    id: String,
    price_id: String,
    display_name: String,
    context_cap_tokens: i32,
    context_source_url: String,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    context_verified_at: Timestamp,
    input_nanos_per_mtok: i64,
    cached_input_nanos_per_mtok: i64,
    output_nanos_per_mtok: i64,
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn model_seeds_preserve_ids_and_update_exact_prices() -> Result<(), Box<dyn std::error::Error>>
{
    let database = TestDatabase::shared();
    let db = database.open(DbRole::Api, &[]).await;
    let id = mint_id();
    let mut connection = db.acquire().await?;
    ModelSeed::new(&id, PROVIDER, &id, 128_000)
        .insert(&mut *connection)
        .await?;
    let first: Row = sqlx::query_as(READ)
        .bind(&id)
        .fetch_one(&mut *connection)
        .await?;
    assert_eq!(first.id, id);
    assert_eq!(first.display_name, id);
    assert_eq!(
        first.context_source_url,
        "https://fixtures.agentsfleet.net/models"
    );
    assert!(first.context_verified_at > VERIFIED);
    assert_eq!(first.input_nanos_per_mtok, 0);
    ModelSeed::new(&mint_id(), PROVIDER, &id, 64_000)
        .rates(6_000_000, 15_000_000, 187_500_000)
        .verified_at(VERIFIED)
        .insert(&mut *connection)
        .await?;
    let updated: Row = sqlx::query_as(READ)
        .bind(&id)
        .fetch_one(&mut *connection)
        .await?;
    assert_eq!(updated.id, id);
    assert_eq!(updated.price_id, first.price_id);
    assert_eq!(updated.context_cap_tokens, 64_000);
    assert_eq!(updated.context_verified_at, VERIFIED);
    assert_eq!(updated.input_nanos_per_mtok, 6_000_000);
    assert_eq!(updated.cached_input_nanos_per_mtok, 15_000_000);
    assert_eq!(updated.output_nanos_per_mtok, 187_500_000);
    sqlx::query(DELETE)
        .bind(&id)
        .execute(&mut *connection)
        .await?;
    drop(connection);
    db.close().await;
    Ok(())
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_refused_seed_rolls_back_the_identity_and_preserves_the_sql_source()
-> Result<(), Box<dyn std::error::Error>> {
    let database = TestDatabase::shared();
    let db = database.open(DbRole::Api, &[]).await;
    let id = mint_id();
    let mut connection = db.acquire().await?;
    let refused = ModelSeed::new(&id, PROVIDER, &id, 128_000)
        .rates(-1, 0, 0)
        .insert(&mut *connection)
        .await;
    assert!(refused.as_ref().is_err_and(|failure| {
        failure
            .source()
            .and_then(|source| source.downcast_ref::<sqlx::Error>())
            .is_some()
    }));
    assert!(
        sqlx::query_as::<_, Row>(READ)
            .bind(&id)
            .fetch_optional(&mut *connection)
            .await?
            .is_none()
    );
    ModelSeed::new(&id, PROVIDER, &id, 128_000)
        .insert(&mut *connection)
        .await?;
    assert_eq!(
        sqlx::query_as::<_, Row>(READ)
            .bind(&id)
            .fetch_one(&mut *connection)
            .await?
            .input_nanos_per_mtok,
        0
    );
    sqlx::query(DELETE)
        .bind(&id)
        .execute(&mut *connection)
        .await?;
    drop(connection);
    db.close().await;
    Ok(())
}
