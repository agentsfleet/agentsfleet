//! Deterministic admissions-ledger fixtures: queued `thread.turns` rows.
//!
//! Shared-database tests scope rows by fleet. Tests asserting deployment-wide
//! candidate or backlog counts use a private database.
#![expect(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]

use af_core::id::Uuid7;

/// Place a durable admission with the fixture's deterministic event clock.
pub(crate) async fn enqueue(
    database: &afd_db::Db,
    fleet: &Uuid7,
    workspace: &Uuid7,
    actor: &str,
    event_type: &str,
    request_json: &str,
    created_at: jiff::Timestamp,
) -> Uuid7 {
    use jiff_sqlx::ToSqlx as _;
    let event = afd_db::test_util::mint_id();
    sqlx::query("INSERT INTO thread.turns (id,thread_id,workspace_id,producer,producer_key,payload_digest,actor,kind,status,request_json,created_at,updated_at) VALUES ($1,$2,$3,'steer',$1,'fixture',$4,$5,$8,$6::jsonb,$7,$7)")
        .bind(&event).bind(fleet)
        .bind(workspace).bind(actor).bind(event_type)
        .bind(request_json).bind(created_at.to_sqlx())
        .bind(af_core::event::status::QUEUED)
        .execute(&mut *database.acquire().await.expect("runtime connection"))
        .await.expect("durable admission");
    event
}
