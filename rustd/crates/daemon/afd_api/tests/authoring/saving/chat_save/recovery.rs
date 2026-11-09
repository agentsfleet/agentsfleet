//! Activation retries must recheck the creator's current workspace access.

use super::super::activity::fault::Refusal;
use super::*;
use af_core::id::Uuid7;

#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_committed_save_waits_for_restored_access_before_activation() {
    let mut case = Case::create().await;
    case.fund().await;
    let (item, _) = case.request().await;
    accept_message(
        &case,
        &item,
        &json!({"message":"Save this setup","save":true,"operation_id":mint()}),
    )
    .await;
    let draft = read(&case, &item).await;
    let (path, token, report) = finish_request(&mut case, &draft).await;
    let fault = Refusal::install(case.base.owner_url(), &case.base.workspace, "setup_ready").await;
    let refused = send(
        &case.router,
        Method::POST,
        &path,
        Some(&token),
        &report.to_string(),
    )
    .await;
    assert_eq!(refused.status(), StatusCode::INTERNAL_SERVER_ERROR);
    fault.assert_fired_once().await;
    drop(fault);
    let saved = state(&case).await;
    assert_eq!(saved.1, "installing");
    assert_eq!(saved.2, 1);
    revoke_access(&mut case).await;
    report_once(&case, &path, &token, &report).await;
    assert_eq!(
        state(&case).await,
        saved,
        "no activation or new setup without access"
    );
    restore_membership(&case).await;
    for _ in 0..2 {
        report_once(&case, &path, &token, &report).await;
    }
    assert_eq!(state(&case).await, (saved.0, "active".to_owned(), 2));
    case.cleanup().await;
}

async fn revoke_access(case: &mut Case) {
    let other_account = mint();
    case.base.own_tenant(&other_account);
    let mut connection = case.base.db.acquire().await.expect("membership connection");
    sqlx::query("INSERT INTO identity.tenants(id,name,created_at,updated_at) VALUES($1,'Separate personal account',now(),now())")
        .bind(&other_account).execute(&mut *connection).await.expect("owned account");
    sqlx::query("UPDATE identity.users SET tenant_id=$2 WHERE id=$1")
        .bind(&case.base.owner)
        .bind(other_account)
        .execute(&mut *connection)
        .await
        .expect("creator no longer owns workspace");
    sqlx::query("DELETE FROM identity.memberships WHERE tenant_id=$1 AND user_id=$2")
        .bind(&case.base.tenant)
        .bind(&case.base.owner)
        .execute(&mut *connection)
        .await
        .expect("revoke workspace membership");
}

async fn restore_membership(case: &Case) {
    sqlx::query("INSERT INTO identity.memberships(id,tenant_id,user_id,role,created_at) VALUES($1,$2,$3,'admin',now())")
        .bind(mint()).bind(&case.base.tenant).bind(&case.base.owner)
        .execute(&mut *case.base.db.acquire().await.expect("membership restore connection"))
        .await.expect("restore actual membership");
}

async fn state(case: &Case) -> (Uuid7, String, i64) {
    sqlx::query_as(
        "SELECT f.id,f.status,(SELECT count(*) FROM fleet.events e WHERE e.fleet_id=f.id)
        FROM fleet.fleets f WHERE f.workspace_id=$1",
    )
    .bind(&case.base.workspace)
    .fetch_one(
        &mut *case
            .base
            .db
            .acquire()
            .await
            .expect("fleet state connection"),
    )
    .await
    .expect("one committed setup")
}
