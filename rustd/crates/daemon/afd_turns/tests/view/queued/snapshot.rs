//! One public thread read cannot split delivery across database snapshots.

use super::*;
use afd_db::config::DbRole;
use sqlx::PgConnection;

const READ_CALLS: &str = "SELECT COALESCE(sum(calls),0)::bigint FROM pg_stat_statements \
    WHERE dbid=(SELECT oid FROM pg_database WHERE datname=current_database()) \
      AND userid=(SELECT oid FROM pg_roles WHERE rolname='agentsfleet_api') \
      AND query ILIKE '%thread.turns%'";
/// A claim's delivered state: the turn stamped with a fence and its event opened, in place.
const DELIVER: &str = "UPDATE thread.turns \
    SET status=$2, opened_at=now(), run_fence=1, updated_at=now() WHERE id=$1";

// Value: protects=one snapshot per page; fails_when=history assembles two reads;
// why_new=sequential identity tests miss split snapshots; seam=none;
// no_claim=one snapshot across different pages or a concurrent-delivery schedule.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn thread_pages_use_one_statement_snapshot_before_and_after_delivery() {
    let (database, db, tenant) = private_database().await;
    let mut owner = db.acquire().await.expect("private owner");
    sqlx::query("CREATE EXTENSION IF NOT EXISTS pg_stat_statements")
        .execute(&mut *owner)
        .await
        .expect("statement counter");
    sqlx::query(SEED_LEDGER)
        .bind(&tenant)
        .bind(jiff_sqlx::ToSqlx::to_sqlx(jiff::Timestamp::constant(
            1_700_000_000,
            0,
        )))
        .bind(1_i32)
        .bind(50_i32)
        .execute(&mut *owner)
        .await
        .expect("one queued head");
    let (workspace, fleet, event): (Uuid7, Uuid7, Uuid7) = sqlx::query_as(
        "SELECT workspace_id,thread_id,id FROM thread.turns WHERE settled_at IS NULL",
    )
    .fetch_one(&mut *owner)
    .await
    .expect("queued identity");
    let runtime = database.open(DbRole::Api, &[]).await;
    let history = History::new(runtime.clone());
    let queued = one_read(&history, &mut owner, &workspace, &fleet, None).await;
    assert_eq!(queued.len(), 1);
    let first = queued.first().expect("queued row");
    assert_eq!(first.row.event_id, event);
    assert_eq!(first.row.status, status::QUEUED);
    let cursor = Cursor::after(first.row.created_at, &event);
    assert_eq!(
        one_read(&history, &mut owner, &workspace, &fleet, Some(&cursor)).await,
        [] as [afd_turns::view::EventDetailRow; 0]
    );
    sqlx::query(DELIVER)
        .bind(&event)
        .bind(status::RECEIVED)
        .execute(&mut *owner)
        .await
        .expect("atomic fixture delivery");
    let delivered = one_read(&history, &mut owner, &workspace, &fleet, None).await;
    assert_eq!(delivered.len(), 1);
    let delivered = delivered.first().expect("delivered row");
    assert_eq!(delivered.row.event_id, event);
    assert_eq!(delivered.row.status, status::RECEIVED);
    drop((history, owner));
    runtime.close().await;
    db.close().await;
    database.cleanup().await;
}

async fn one_read(
    history: &History,
    owner: &mut PgConnection,
    workspace: &Uuid7,
    fleet: &Uuid7,
    cursor: Option<&Cursor>,
) -> Vec<afd_turns::view::EventDetailRow> {
    let before: i64 = sqlx::query_scalar(READ_CALLS)
        .fetch_one(&mut *owner)
        .await
        .expect("before read counter");
    let page = history
        .thread_page(workspace, fleet, cursor, PAGE)
        .await
        .expect("public history read");
    let after: i64 = sqlx::query_scalar(READ_CALLS)
        .fetch_one(owner)
        .await
        .expect("after read counter");
    assert_eq!(
        after - before,
        1,
        "one statement provides one snapshot, cursor={}",
        cursor.is_some()
    );
    page
}
