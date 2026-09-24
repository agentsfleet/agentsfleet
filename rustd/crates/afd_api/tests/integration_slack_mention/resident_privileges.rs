//! The resident bind, run as the role a request meets.
//!
//! The lane connects as the database owner, which bypasses grants, so every
//! other case in this suite stayed green while `api_runtime` could not run the
//! bind at all: `INSERT_RESIDENT` re-points a moved team's binding with
//! `ON CONFLICT … DO UPDATE`, Postgres asks for UPDATE on the columns it
//! assigns whether or not a row conflicts, and slot 560 granted only SELECT and
//! INSERT. The case here takes `api_runtime` inside a transaction that rolls
//! back, and asserts on what Postgres then does.

#![cfg(feature = "test-util")]

use afd_db::test_util::mint_id;
use afd_ingress::slack::KIND_RESIDENT;
use afd_ingress::sql;

use super::resident_bound::{bind, resident_document, resident_name};
use super::*;

/// The role every request's statements run under.
const RUNTIME_ROLE: &str = "SET LOCAL ROLE api_runtime";

/// A moved team's binding is re-pointed by the runtime role, not only by the
/// owner the lane connects as.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_moved_teams_binding_is_repointed_as_api_runtime() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let name = resident_name(&fixture);
    let left_behind = fixture
        .fleet_elsewhere(&resident_document(&name), FleetStatus::Active.as_str())
        .await;
    bind(&fixture, &left_behind).await;
    let resident = fixture
        .fleet(&resident_document(&name), FleetStatus::Active.as_str())
        .await;

    let mut connection = fixture.database().acquire().await.expect("a connection");
    let mut transaction = sqlx::Connection::begin(&mut *connection)
        .await
        .expect("the probe transaction opens");
    sqlx::query(RUNTIME_ROLE)
        .execute(&mut *transaction)
        .await
        .expect("the lane's user may assume api_runtime");
    let repointed: Option<String> = sqlx::query_scalar(sql::INSERT_RESIDENT)
        .bind(mint_id())
        .bind(PROVIDER.id())
        .bind(&fixture.team)
        .bind(CHANNEL)
        .bind(resident.as_str())
        .bind(KIND_RESIDENT)
        .bind(1_i64)
        .bind(fixture.workspace().as_str())
        .fetch_optional(&mut *transaction)
        .await
        .expect("api_runtime runs the resident bind, re-point included");
    transaction
        .rollback()
        .await
        .expect("the probe transaction rolls back");
    drop(connection);

    assert_eq!(
        repointed.as_deref(),
        Some(resident.as_str()),
        "the binding names this workspace's resident, not the one left behind"
    );

    fixture.cleanup().await;
}
