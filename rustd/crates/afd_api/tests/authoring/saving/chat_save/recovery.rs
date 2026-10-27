//! Activation retries must recheck the creator's current workspace access.

use super::super::activity::fault::Refusal;
use super::*;

#[tokio::test]
#[ignore = "needs isolated local Postgres and Dragonfly"]
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
    sqlx::query("INSERT INTO identity.tenants(id,name,created_at,updated_at) VALUES($1::uuid,'Separate personal account',now(),now())")
        .bind(other_account.as_str()).execute(&mut *connection).await.expect("owned account");
    sqlx::query("UPDATE identity.users SET tenant_id=$2::uuid WHERE id=$1::uuid")
        .bind(case.base.owner.as_str())
        .bind(other_account.as_str())
        .execute(&mut *connection)
        .await
        .expect("creator no longer owns workspace");
    sqlx::query("DELETE FROM identity.memberships WHERE tenant_id=$1::uuid AND user_id=$2::uuid")
        .bind(case.base.tenant.as_str())
        .bind(case.base.owner.as_str())
        .execute(&mut *connection)
        .await
        .expect("revoke workspace membership");
}

async fn restore_membership(case: &Case) {
    sqlx::query("INSERT INTO identity.memberships(id,tenant_id,user_id,role,created_at) VALUES($1::uuid,$2::uuid,$3::uuid,'admin',now())")
        .bind(mint().as_str()).bind(case.base.tenant.as_str()).bind(case.base.owner.as_str())
        .execute(&mut *case.base.db.acquire().await.expect("membership restore connection"))
        .await.expect("restore actual membership");
}

async fn state(case: &Case) -> (String, String, i64) {
    sqlx::query_as(
        "SELECT f.id::text,f.status,(SELECT count(*) FROM fleet.events e WHERE e.fleet_id=f.id)
        FROM fleet.fleets f WHERE f.workspace_id=$1::uuid",
    )
    .bind(case.base.workspace.as_str())
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
