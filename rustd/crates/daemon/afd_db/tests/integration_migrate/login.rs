//! The API login carries the intake bounds its role was provisioned with.

use afd_db::login::{LOCK_TIMEOUT, STATEMENT_TIMEOUT};

use super::*;

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn the_api_login_runs_under_the_intake_bounds() {
    let lane = TestDatabase::shared();
    let api = lane.open(DbRole::Api, &[]).await;
    let mut connection = api.acquire().await.unwrap();
    let (lock, statement): (bool, bool) = sqlx::query_as(
        "SELECT current_setting('lock_timeout')::interval = $1::interval, \
                current_setting('statement_timeout')::interval = $2::interval",
    )
    .bind(LOCK_TIMEOUT)
    .bind(STATEMENT_TIMEOUT)
    .fetch_one(&mut *connection)
    .await
    .unwrap();
    assert!(lock, "the API login's lock_timeout is {LOCK_TIMEOUT}");
    assert!(
        statement,
        "the API login's statement_timeout is {STATEMENT_TIMEOUT}"
    );
    drop(connection);
    api.close().await;
}
