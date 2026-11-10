//! Immutable parent locks run as the API role without granting mutation rights.

use af_core::id::Uuid7;
use afd_db::test_util::{FleetSeed, TenantSeed, WorkspaceSeed, mint_id};
use afd_fleet::task::{
    CHECKPOINT_WORKSPACE_LOCK_STATEMENT, TASK_COMPLETION_COLLECTION_STATEMENT,
    TASK_SCOPE_LOCK_STATEMENT,
};
use sqlx::Connection as _;

use super::*;

struct Scope {
    workspace: Uuid7,
    other: Uuid7,
    unfinished: Uuid7,
    fleet: Uuid7,
}

async fn seed(db: &afd_db::Db) -> Scope {
    let tenant = mint_id();
    let scope = Scope {
        workspace: mint_id(),
        other: mint_id(),
        unfinished: mint_id(),
        fleet: mint_id(),
    };
    let mut connection = db.acquire().await.expect("fixture connection");
    TenantSeed::new(&tenant)
        .insert(&mut *connection)
        .await
        .expect("tenant");
    for workspace in [&scope.workspace, &scope.other, &scope.unfinished] {
        WorkspaceSeed::new(workspace, &tenant)
            .insert(&mut *connection)
            .await
            .expect("workspace");
    }
    FleetSeed::new(&scope.fleet, &scope.workspace, &tenant, "active")
        .insert(&mut *connection)
        .await
        .expect("fleet");
    scope
}

async fn api(connection: &mut PgConnection) {
    sqlx::query("SET LOCAL ROLE api_runtime")
        .execute(connection)
        .await
        .expect("real API privileges");
}

async fn cannot_update(connection: &mut PgConnection) {
    let mut savepoint = connection.begin().await.expect("privilege savepoint");
    let failure = sqlx::query("UPDATE identity.workspaces SET name = name WHERE false")
        .execute(&mut *savepoint)
        .await
        .expect_err("workspace UPDATE remains forbidden");
    assert_eq!(
        failure
            .as_database_error()
            .and_then(sqlx::error::DatabaseError::code)
            .as_deref(),
        Some("42501")
    );
    savepoint
        .rollback()
        .await
        .expect("restore transaction after denied UPDATE");
}

// Value: protects=API lease and checkpoint transactions hold exact immutable parents;
// fails_when=runtime needs UPDATE privilege, loses its deletion fence, or locks unrelated workspaces;
// why_new=table grant probes did not exercise row locks; seam=exported production SQL;
// no_claim=full lease issuance or remote object storage.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn api_parent_locks_preserve_immutability_and_exact_scope() {
    let (lane, db) = migrated().await;
    let scope = seed(&db).await;
    let mut connection = db.acquire().await.expect("lock owner");
    let mut held = connection.begin().await.expect("parent transaction");
    api(&mut held).await;
    let locked: Uuid7 = sqlx::query_scalar(TASK_SCOPE_LOCK_STATEMENT)
        .bind(&scope.fleet)
        .bind(&scope.workspace)
        .fetch_one(&mut *held)
        .await
        .expect("API scope lock");
    assert_eq!(locked, scope.fleet);
    let missing: Option<Uuid7> = sqlx::query_scalar(TASK_SCOPE_LOCK_STATEMENT)
        .bind(&scope.fleet)
        .bind(mint_id())
        .fetch_optional(&mut *held)
        .await
        .expect("absent scope");
    assert_eq!(missing, None);
    cannot_update(&mut held).await;
    let mut contender = db.acquire().await.expect("independent deleter");
    blocked_parent_and_unrelated_delete(&mut contender, &scope).await;
    held.commit().await.expect("release scope locks");

    let mut checkpoint = connection.begin().await.expect("checkpoint transaction");
    api(&mut checkpoint).await;
    let locked: Uuid7 = sqlx::query_scalar(CHECKPOINT_WORKSPACE_LOCK_STATEMENT)
        .bind(&scope.workspace)
        .fetch_one(&mut *checkpoint)
        .await
        .expect("API checkpoint parent lock");
    assert_eq!(locked, scope.workspace);
    cannot_update(&mut checkpoint).await;
    assert_parent_locked(&mut contender, &scope.workspace).await;
    checkpoint.commit().await.expect("release checkpoint lock");
    assert_eq!(
        sqlx::query("DELETE FROM identity.workspaces WHERE id=$1")
            .bind(&scope.workspace)
            .execute(&mut *contender)
            .await
            .expect("deletion proceeds after commit")
            .rows_affected(),
        1
    );
    drop(contender);
    drop(connection);
    db.close().await;
    lane.cleanup().await;
}

async fn completions(
    connection: &mut PgConnection,
    workspace: &Uuid7,
    count: usize,
    age: Duration,
    finished: bool,
) -> Vec<Uuid7> {
    let ids: Vec<_> = (0..count).map(|_| mint_id()).collect();
    sqlx::query("INSERT INTO fleet.task_completions (id, workspace_id, installation_id, repository_id, head_oid, workflow_id, run_id, run_number, run_attempt, conclusion, body_digest, received_at, created_at) SELECT id, $1, 1, 1, repeat('a',40), 1, number, number, 1, 'success', repeat('b',64), statement_timestamp()-$3::interval, statement_timestamp() FROM unnest($2) WITH ORDINALITY AS owned(id,number)")
        .bind(workspace).bind(&ids).bind(age).execute(&mut *connection).await.expect("owned immutable completions");
    sqlx::query("INSERT INTO fleet.task_completion_fanout (task_completion_id, next_attempt_at, finished_at, created_at, updated_at) SELECT id, statement_timestamp(), CASE WHEN $2 THEN statement_timestamp() ELSE NULL END, statement_timestamp(), statement_timestamp() FROM unnest($1) AS owned(id)")
        .bind(&ids).bind(finished).execute(connection).await.expect("owned fanout custody");
    ids
}

async fn collect(
    connection: &mut PgConnection,
    retention: Option<&str>,
    limit: Option<i64>,
) -> u64 {
    // The native statement takes an interval. Cast the test's optional textual
    // duration through PostgreSQL so NULL/negative policy probes reach the helper.
    let retention: Option<sqlx::postgres::types::PgInterval> =
        sqlx::query_scalar("SELECT $1::text::interval")
            .bind(retention)
            .fetch_one(&mut *connection)
            .await
            .expect("retention parameter");
    sqlx::query(TASK_COMPLETION_COLLECTION_STATEMENT)
        .bind(retention)
        .bind(limit)
        .execute(connection)
        .await
        .expect("API completion collection")
        .rows_affected()
}

// Value: protects=bounded parent-first GC on immutable receipts under real runtime grants;
// fails_when=GC needs UPDATE, waits on a contended parent, collects live/unfinished receipts, or accepts unbounded inputs;
// why_new=role table probes do not execute the GC lock clause; seam=exported production SQL;
// no_claim=provider ingress or fanout signal delivery.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn api_completion_gc_skips_held_parents_and_enforces_policy_bounds() {
    let (lane, db) = migrated().await;
    let scope = seed(&db).await;
    let mut connection = db.acquire().await.expect("completion owner");
    let old = seed_receipts(&mut connection, &scope).await;
    let mut holder = db.acquire().await.expect("parent contender");
    let mut held = holder
        .begin()
        .await
        .expect("hold completion before collection");
    let first = old.first().expect("seeded old receipt");
    sqlx::query("SELECT id FROM fleet.task_completions WHERE id=$1 FOR UPDATE")
        .bind(first)
        .execute(&mut *held)
        .await
        .expect("hold immutable parent as owner");
    let mut collector = connection.begin().await.expect("collector transaction");
    api(&mut collector).await;
    invalid_collection_inputs(&mut collector).await;
    let collected = tokio::time::timeout(
        Duration::from_secs(5),
        collect(&mut collector, Some("1 hour"), Some(i64::MAX)),
    )
    .await
    .expect("skip held parent without waiting");
    assert_eq!(collected, 256);
    collector.commit().await.expect("commit bounded collection");
    held.rollback().await.expect("release retained parent");
    let mut collector = connection.begin().await.expect("next collection");
    api(&mut collector).await;
    assert_eq!(collect(&mut collector, Some("1 hour"), Some(1)).await, 1);
    assert_eq!(collect(&mut collector, Some("1 hour"), Some(1)).await, 1);
    assert_eq!(collect(&mut collector, Some("1 hour"), Some(1)).await, 0);
    collector.commit().await.expect("finish collection");
    let recent: i64 =
        sqlx::query_scalar("SELECT count(*) FROM fleet.task_completions WHERE workspace_id=$1")
            .bind(&scope.other)
            .fetch_one(&mut *connection)
            .await
            .expect("recent receipt survives");
    assert_eq!(recent, 1);
    let unfinished: i64 =
        sqlx::query_scalar("SELECT count(*) FROM fleet.task_completions WHERE workspace_id=$1")
            .bind(&scope.unfinished)
            .fetch_one(&mut *connection)
            .await
            .expect("unfinished receipt survives");
    assert_eq!(unfinished, 1);
    drop(holder);
    drop(connection);
    db.close().await;
    lane.cleanup().await;
}

async fn blocked_parent_and_unrelated_delete(connection: &mut PgConnection, scope: &Scope) {
    assert_parent_locked(connection, &scope.workspace).await;
    assert_eq!(
        sqlx::query("DELETE FROM identity.workspaces WHERE id=$1")
            .bind(&scope.other)
            .execute(&mut *connection)
            .await
            .expect("unrelated deletion proceeds")
            .rows_affected(),
        1
    );
}

async fn seed_receipts(connection: &mut PgConnection, scope: &Scope) -> Vec<Uuid7> {
    let old = completions(
        connection,
        &scope.workspace,
        258,
        Duration::from_hours(96),
        true,
    )
    .await;
    // Other workspaces avoid colliding with the immutable run identity key.
    let _recent = completions(connection, &scope.other, 1, Duration::from_hours(71), true).await;
    let _unfinished = completions(
        connection,
        &scope.unfinished,
        1,
        Duration::from_hours(96),
        false,
    )
    .await;
    old
}

async fn invalid_collection_inputs(connection: &mut PgConnection) {
    for (retention, limit) in [
        (None, Some(1)),
        (Some("-1 hour"), Some(1)),
        (Some("1 hour"), None),
        (Some("1 hour"), Some(-1)),
    ] {
        assert_eq!(collect(connection, retention, limit).await, 0);
    }
}

async fn assert_parent_locked(connection: &mut PgConnection, workspace: &Uuid7) {
    let mut deletion = connection.begin().await.expect("deletion attempt");
    let blocked = sqlx::query("SELECT id FROM identity.workspaces WHERE id=$1 FOR UPDATE NOWAIT")
        .bind(workspace)
        .fetch_one(&mut *deletion)
        .await
        .expect_err("workspace deletion fence");
    assert_eq!(
        blocked
            .as_database_error()
            .and_then(sqlx::error::DatabaseError::code)
            .as_deref(),
        Some("55P03")
    );
    deletion.rollback().await.expect("release failed contender");
}
