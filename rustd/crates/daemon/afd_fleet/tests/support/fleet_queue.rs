//! Deterministic admissions-ledger fixtures.
//!
//! Shared-database tests scope rows by fleet. Tests asserting deployment-wide
//! candidate or backlog counts use a private database.
#![allow(
    dead_code,
    reason = "test support: shared by several test binaries, each using a subset"
)]
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
    sqlx::query("INSERT INTO fleet.admissions (id,fleet_id,workspace_id,producer,producer_key,payload_digest,actor,event_type,request_json,event_created_at,created_at,updated_at) VALUES ($1,$2,$3,'steer',$1,'fixture',$4,$5,$6,$7,$7,$7)")
        .bind(&event).bind(fleet)
        .bind(workspace).bind(actor).bind(event_type)
        .bind(request_json).bind(created_at.to_sqlx())
        .execute(&mut *database.acquire().await.expect("runtime connection"))
        .await.expect("durable admission");
    event
}
