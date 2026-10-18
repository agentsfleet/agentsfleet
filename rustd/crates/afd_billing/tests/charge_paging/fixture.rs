use afd_db::test_util::mint_id;
use afd_db::{Db, MIGRATIONS, Migration};

pub(super) fn through(slot: i32) -> &'static [Migration] {
    let end = MIGRATIONS.iter().position(|m| m.version() == slot).unwrap();
    MIGRATIONS.get(..=end).unwrap()
}

pub(super) async fn seed(db: &Db) -> String {
    let tenant = mint_id();
    let mut connection = db.acquire().await.unwrap();
    sqlx::query(
        "INSERT INTO core.tenants(id,name,created_at,updated_at) VALUES($1::uuid,'paging',1,1)",
    )
    .bind(&tenant)
    .execute(&mut *connection)
    .await
    .unwrap();
    sqlx::query("INSERT INTO core.workspaces(id,tenant_id,created_at) VALUES($1::uuid,$1::uuid,1)")
        .bind(&tenant)
        .execute(&mut *connection)
        .await
        .unwrap();
    sqlx::query("INSERT INTO fleet.runners(id,host_id,token_hash,sandbox_tier,admin_state,labels,network_policy,registry_allowlist,worker_count,degraded,last_seen_at,created_at,updated_at) VALUES($1::uuid,$1,$1,'dev_none','active','[]','deny_all_egress','[]',1,false,1,1,1)")
        .bind(&tenant).execute(&mut *connection).await.unwrap();
    tenant
}

pub(super) async fn attempts(db: &Db, tenant: &str, start: i64, end: i64) {
    sqlx::query("WITH ids AS (SELECT overlay(overlay(md5($1||i::text) placing '7' from 13) placing '8' from 17)::uuid AS id,i FROM generate_series($2::bigint,$3::bigint) i) INSERT INTO fleet.authoring_attempts(id,lease_id,runner_id,workspace_id,tenant_id,creator_id,draft_id,message_sequence,fence,deletion_generation,capability_hash,model_snapshot,status,lease_expires_at,deadline,reserved_nanos,settled_nanos,usage_unknown,calls,input_tokens,output_tokens,activity_sequence,created_at,updated_at) SELECT id,id,$1::uuid,$1::uuid,$1::uuid,$1::uuid,$1::uuid,i,1,0,id::text,'{\"model\":\"setup-model\",\"platform_managed\":true}','settled',1,1,17,17,false,1,23,29,0,i,i FROM ids")
        .bind(tenant).bind(start).bind(end)
        .execute(&mut *db.acquire().await.unwrap()).await.unwrap();
}

pub(super) async fn settlements(db: &Db, tenant: &str, start: i64, end: i64) {
    sqlx::query("INSERT INTO billing.authoring_settlements(id,attempt_id,report_id,accounting_digest,outcome,usage,charged_nanos,created_at) SELECT id,id,id,id::text,'completed','{\"input_tokens\":23,\"output_tokens\":29,\"runtime_ms\":31}',17,created_at FROM fleet.authoring_attempts WHERE tenant_id=$1::uuid AND created_at BETWEEN $2 AND $3")
        .bind(tenant).bind(start).bind(end)
        .execute(&mut *db.acquire().await.unwrap()).await.unwrap();
}

pub(super) async fn snapshot(db: &Db, tenant: &str) -> serde_json::Value {
    sqlx::query_scalar("SELECT jsonb_agg(to_jsonb(s) - 'tenant_id' ORDER BY s.id) FROM billing.authoring_settlements s JOIN fleet.authoring_attempts a ON a.id=s.attempt_id WHERE a.tenant_id=$1::uuid")
        .bind(tenant).fetch_one(&mut *db.acquire().await.unwrap()).await.unwrap()
}
