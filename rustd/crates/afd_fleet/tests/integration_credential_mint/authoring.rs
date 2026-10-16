//! An authoring lease carries model access and nothing else: the runner that
//! holds one cannot mint a fleet credential with it, even beside a fleet lease
//! that can.

use super::*;
use crate::requests;

/// A live authoring attempt held by the same runner, in the same workspace.
const AUTHORING: &str = "\
INSERT INTO fleet.authoring_attempts(id,lease_id,runner_id,workspace_id,tenant_id,creator_id,
draft_id,message_sequence,fence,deletion_generation,capability_hash,model_snapshot,status,
lease_expires_at,deadline,reserved_nanos,settled_nanos,usage_unknown,calls,input_tokens,
output_tokens,activity_sequence,created_at,updated_at)
SELECT $1::uuid,$2::uuid,$3::uuid,f.workspace_id,f.tenant_id,$1::uuid,$1::uuid,1,1,0,$1,'{}',
$5,$6,$6,0,0,false,0,0,0,0,$7,$7 FROM core.fleets f WHERE f.id=$4::uuid";

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires a live Postgres; run through `make test-integration-rustd`"]
async fn test_an_authoring_lease_has_no_credential_mint_rights() {
    support::install_subscriber();
    let fixtures = Fixtures::create_with_queue().await;
    let owner = bound(&fixtures, "{}").await;
    seed_handle(
        &fixtures,
        &owner.workspace,
        CONNECTOR_STATIC,
        &format!(r#"{{"integration":"static","token":"{OWNER_TOKEN}"}}"#),
    )
    .await;
    let attempt = new_id();
    let authoring = new_id();
    seed_authoring(&fixtures, &owner, &attempt, &authoring).await;
    let now = UnixMillis::from_millis(NOW_MS);
    let plane = fixtures.plane();

    let refused = plane
        .mint(
            &owner.runner,
            &requests::mint(&authoring, CONNECTOR_STATIC),
            now,
        )
        .await
        .expect_err("an authoring lease is no mint authority");
    assert_eq!(
        refused.code(),
        afd_fleet::Error::lease_not_found().code(),
        "the authoring lease reads exactly like a lease that does not exist"
    );
    assert!(
        fixtures
            .leases()
            .mint_scope(&owner.runner, &authoring, now)
            .await
            .expect("the read must succeed")
            .is_none(),
        "a live authoring lease resolved to a fleet scope"
    );

    let minted = plane
        .mint(
            &owner.runner,
            &requests::mint(&owner.lease, CONNECTOR_STATIC),
            now,
        )
        .await
        .expect("the same runner's fleet lease still mints");
    assert_eq!(minted.token.as_str(), OWNER_TOKEN);

    remove_authoring(&fixtures, &attempt).await;
    fixtures.cleanup().await;
}

async fn seed_authoring(fixtures: &Fixtures, owner: &Bound, attempt: &str, lease: &str) {
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    let inserted = sqlx::query(AUTHORING)
        .bind(attempt)
        .bind(lease)
        .bind(owner.runner.as_str())
        .bind(&owner.fleet)
        .bind(afd_runner::sql::LEASE_STATUS_ACTIVE)
        .bind(NOW_MS + LEASE_WINDOW_MS)
        .bind(NOW_MS)
        .execute(&mut *connection)
        .await
        .expect("the live authoring attempt must insert");
    assert_eq!(inserted.rows_affected(), 1, "the fleet's tenant owns it");
}

async fn remove_authoring(fixtures: &Fixtures, attempt: &str) {
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query("DELETE FROM fleet.authoring_attempts WHERE id = $1::uuid")
        .bind(attempt)
        .execute(&mut *connection)
        .await
        .expect("remove only this test's attempt");
}
