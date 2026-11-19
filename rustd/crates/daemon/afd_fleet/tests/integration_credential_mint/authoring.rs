//! An authoring run carries model access and nothing else: the runner that
//! holds one cannot mint a fleet credential with it, even beside a fleet run
//! that can.

use super::*;
use crate::requests;
use afd_db::config::DbRole;

/// A live authoring attempt held by the same runner, in the same workspace.
const AUTHORING: &str = "\
INSERT INTO authoring.attempts(id,run_id,runner_id,workspace_id,tenant_id,user_id,
draft_id,message_sequence,fencing_token,model_token_hash,model_snapshot,status,
run_expires_at,expires_at,reserved_nanos,usage_unknown,call_count,output_token_count,
reported_runtime_ms,last_activity_sequence,created_at,updated_at)
SELECT $1,$2,$3,f.workspace_id,f.tenant_id,$1,$1,1,1,$1,'{}',
$5,now() + $6::interval,now() + $6::interval,0,false,0,0,0,0,now(),now()
FROM fleet.fleets f WHERE f.id=$4";

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires a live Postgres and NATS; run through `make test-integration-rustd`"]
async fn test_an_authoring_run_has_no_credential_mint_rights() {
    support::install_subscriber();
    let fixtures = Fixtures::create().await;
    let owner = bound(&fixtures, "{}").await;
    seed_handle(
        &fixtures,
        &owner.workspace,
        CONNECTOR_STATIC,
        &format!(r#"{{"integration":"static","token":"{OWNER_TOKEN}"}}"#),
    )
    .await;
    let attempt = mint_id();
    let authoring = mint_id();
    seed_authoring(&fixtures, &owner, &attempt, &authoring).await;
    let now = fixtures.enrolled_at();
    let plane = fixtures.plane();

    let refused = plane
        .mint(
            &owner.runner,
            &requests::mint(authoring.as_str(), CONNECTOR_STATIC),
            now,
        )
        .await
        .expect_err("an authoring run is no mint authority");
    assert_eq!(
        refused.code(),
        afd_fleet::Error::run_not_found().code(),
        "the authoring run reads exactly like a run that does not exist"
    );
    assert!(
        fixtures
            .runs()
            .mint_scope(&owner.runner, &authoring)
            .await
            .expect("the read must succeed")
            .is_none(),
        "a live authoring run resolved to a fleet scope"
    );

    let minted = plane
        .mint(
            &owner.runner,
            &requests::mint(owner.run.as_str(), CONNECTOR_STATIC),
            now,
        )
        .await
        .expect("the same runner's fleet run still mints");
    assert_eq!(minted.token.as_str(), OWNER_TOKEN);

    remove_authoring(&fixtures, &attempt).await;
    fixtures.cleanup().await;
}

async fn seed_authoring(fixtures: &Fixtures, owner: &Bound, attempt: &Uuid7, run: &Uuid7) {
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    let inserted = sqlx::query(AUTHORING)
        .bind(attempt)
        .bind(run)
        .bind(&owner.runner)
        .bind(&owner.fleet)
        .bind(afd_runner::sql::RUN_STATUS_ACTIVE)
        .bind(RUN_WINDOW)
        .execute(&mut *connection)
        .await
        .expect("the live authoring attempt must insert");
    assert_eq!(inserted.rows_affected(), 1, "the fleet's tenant owns it");
}

/// Cleanup through the lane's owner: `api_runtime` never deletes an attempt,
/// and the fixture must not borrow a right production does not have.
async fn remove_authoring(fixtures: &Fixtures, attempt: &Uuid7) {
    let owner = fixtures.open(DbRole::Migrator).await;
    let mut connection = owner.acquire().await.expect("an owner connection");
    sqlx::query("DELETE FROM setup.attempts WHERE id = $1")
        .bind(attempt)
        .execute(&mut *connection)
        .await
        .expect("remove only this test's attempt");
    drop(connection);
    owner.close().await;
}
