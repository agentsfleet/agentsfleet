use afd_db::Db;
use afd_db::test_util::mint_id;

/// A tenant identifier the history is written under. Attempts and usage lines
/// keep opaque owner identifiers (deletion matrix), so no tenant row is needed.
pub(super) fn tenant() -> String {
    mint_id()
}

/// Settled setup attempts `start..=end` for `tenant`, the `i`th created at
/// second `i`.
pub(super) async fn attempts(db: &Db, tenant: &str, start: i64, end: i64) {
    sqlx::query("WITH ids AS (SELECT overlay(overlay(md5($1||i::text) placing '7' from 13) placing '8' from 17)::uuid AS id,i FROM generate_series($2::bigint,$3::bigint) i) INSERT INTO authoring.attempts(id,lease_id,runner_id,workspace_id,tenant_id,user_id,draft_id,message_sequence,fencing_token,model_token_hash,model_snapshot,status,lease_expires_at,expires_at,reserved_nanos,usage_unknown,call_count,output_token_count,reported_runtime_ms,last_activity_sequence,created_at,updated_at) SELECT id,id,NULL,$1::uuid,$1::uuid,$1::uuid,$1::uuid,i,1,id::text,'{\"model\":\"setup-model\",\"platform_managed\":true}','settled',to_timestamp(i),to_timestamp(i),17,false,1,29,31,0,to_timestamp(i),to_timestamp(i) FROM ids")
        .bind(tenant).bind(start).bind(end)
        .execute(&mut *db.acquire().await.unwrap()).await.unwrap();
}

/// One settled setup usage line per attempt created at seconds `start..=end`,
/// as a setup chat's report logs it.
pub(super) async fn lines(db: &Db, tenant: &str, start: i64, end: i64) {
    sqlx::query("INSERT INTO billing.usage_lines(id,work_type,work_id,seq,tenant_id,workspace_id,authoring_attempt_id,provider,model,posture,requested_at,input_token_count,cached_input_token_count,cache_write_token_count,cache_write_5m_token_count,cache_write_1h_token_count,output_token_count,runtime_ms,unpriced_token_count,charged_nanos,settled_at,created_at,updated_at) SELECT id,$4,id,0,tenant_id,workspace_id,id,'setup','setup-model','platform',created_at,23,0,0,0,0,29,31,0,17,created_at,created_at,created_at FROM authoring.attempts WHERE tenant_id=$1::uuid AND created_at BETWEEN to_timestamp($2) AND to_timestamp($3)")
        .bind(tenant).bind(start).bind(end).bind(afd_billing::sql::work::AUTHORING_ATTEMPT)
        .execute(&mut *db.acquire().await.unwrap()).await.unwrap();
}
