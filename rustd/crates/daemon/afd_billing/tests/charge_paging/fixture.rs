use af_core::id::Uuid7;
use afd_db::Db;
use afd_db::test_util::mint_id;

/// A tenant identifier the history is written under. Usage lines keep
/// opaque owner identifiers (deletion matrix), so no tenant row is needed.
pub(super) fn tenant() -> Uuid7 {
    mint_id()
}

/// One settled setup usage line per second `start..=end` for `tenant`, the
/// `i`th created at second `i`, as a setup run's report logs it: its work
/// and run are an id derived from the tenant and `i`.
pub(super) async fn lines(db: &Db, tenant: &Uuid7, start: i64, end: i64) {
    sqlx::query("WITH ids AS (SELECT overlay(overlay(md5($1||i::text) placing '7' from 13) placing '8' from 17)::uuid AS id,i FROM generate_series($2::bigint,$3::bigint) i) INSERT INTO billing.usage_lines(id,work_type,work_id,seq, kind,tenant_id,workspace_id,run_id,provider,model,posture,requested_at,input_token_count,cached_input_token_count,cache_write_token_count,cache_write_5m_token_count,cache_write_1h_token_count,output_token_count,runtime_ms,unpriced_token_count,charged_nanos,settled_at,created_at,updated_at) SELECT id,$4,id,0, 'model',$1,$1,id,'setup','setup-model','platform',to_timestamp(i),23,0,0,0,0,29,31,0,17,to_timestamp(i),to_timestamp(i),to_timestamp(i) FROM ids")
        .bind(tenant).bind(start).bind(end).bind(afd_billing::sql::work::SETUP_RUN)
        .execute(&mut *db.acquire().await.unwrap()).await.unwrap();
}
