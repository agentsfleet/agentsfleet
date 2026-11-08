//! A `uuid` read over Postgres's text protocol decodes as a `Uuid7` too.
//!
//! Prepared statements answer in binary, which every other suite covers.
//! `sqlx::raw_sql` sends a simple query, and its rows come back as text.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use af_core::id::Uuid7;
use afd_db::config::DbRole;
use afd_db::test_util::TestDatabase;
use sqlx::FromRow as _;

const MINTED: &str = "0197a4ba-8d3a-7f13-8abc-123456789abc";

#[derive(Debug, sqlx::FromRow)]
struct Minted {
    id: Uuid7,
}

/// The text form decodes to the same identifier, and a version-4 value is
/// refused rather than passed on as an id this daemon never minted.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_uuid_in_text_form_decodes_and_a_v4_is_refused() {
    let lane = TestDatabase::shared();
    let database = lane.open(DbRole::Api, &[]).await;
    let mut connection = database.acquire().await.expect("an API connection");
    let row = sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "SELECT '{MINTED}'::uuid AS id"
    )))
    .fetch_one(&mut *connection)
    .await
    .expect("the text-protocol read answers");
    let minted = Minted::from_row(&row).expect("the text form decodes");
    assert_eq!(minted.id, *MINTED);

    let row = sqlx::raw_sql("SELECT gen_random_uuid() AS id")
        .fetch_one(&mut *connection)
        .await
        .expect("the text-protocol read answers");
    assert!(Minted::from_row(&row).is_err(), "a v4 value is no Uuid7");
    drop(connection);
    database.close().await;
}
