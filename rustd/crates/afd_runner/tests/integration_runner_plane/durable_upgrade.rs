//! A forward upgrade enables existing hosts without overwriting later choices.

use afd_db::{
    Db, Migrator,
    config::DbRole,
    migration::MIGRATIONS,
    test_util::{TestDatabase, mint_id},
};

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn upgrade_enables_existing_and_new_hosts_and_preserves_an_explicit_disabled_choice() {
    let lane = TestDatabase::create().await;
    let database = lane.open(DbRole::Migrator, &[]).await;
    let boundary = MIGRATIONS
        .iter()
        .position(|migration| migration.name() == "2690_runner_durable_tasks.sql")
        .expect("policy migration registered");
    let before = MIGRATIONS
        .get(..boundary)
        .expect("preference follows runner table");
    Migrator::new()
        .with_migrations(before)
        .run(&database)
        .await
        .expect("prior schema migrates");
    let existing = mint_id();
    insert_host(&database, &existing, &"a".repeat(64)).await;
    Migrator::new()
        .run(&database)
        .await
        .expect("existing host upgrades");
    let fresh = mint_id();
    insert_host(&database, &fresh, &"b".repeat(64)).await;
    disable_after_defaults(&database, &existing, &fresh).await;
    Migrator::new()
        .run(&database)
        .await
        .expect("idempotent upgrade");
    let mut connection = database.acquire().await.expect("read retained policy");
    let retained: bool =
        sqlx::query_scalar("SELECT durable_tasks_enabled FROM runner.runners WHERE id = $1::uuid")
            .bind(existing.as_str())
            .fetch_one(&mut *connection)
            .await
            .expect("stored choice");
    assert!(!retained);
    drop(connection);
    database.close().await;
    lane.cleanup().await;
}

async fn insert_host(database: &Db, id: &str, digest: &str) {
    let mut connection = database.acquire().await.expect("host seed connection");
    let tier = afd_core::spelling::to_spelling(&afd_wire::runner::SandboxTier::DevNone)
        .expect("wire tier spelling");
    let network = afd_core::spelling::to_spelling(&afd_wire::runner::NetworkPolicy::AllowAll)
        .expect("wire network spelling");
    sqlx::query("INSERT INTO runner.runners (id, host_id, token_hash, sandbox_tier, status, labels, network_policy, registry_allowlist, worker_count, last_seen_at, lease_version, created_at, updated_at) VALUES ($1::uuid, $1::text, $2, $3, $4, '[]'::jsonb, $5, '[]'::jsonb, $6, now(), $7, now(), now())")
        .bind(id).bind(digest).bind(tier).bind(afd_runner::sql::RUNNER_STATUS_ACTIVE)
        .bind(network).bind(1_i32).bind(0_i64).execute(&mut *connection).await.expect("host before or after additive migration");
}

async fn disable_after_defaults(database: &Db, existing: &str, fresh: &str) {
    let mut connection = database.acquire().await.expect("upgraded datastore");
    let defaults: Vec<bool> = sqlx::query_scalar(
        "SELECT durable_tasks_enabled FROM runner.runners WHERE id = ANY(($1::text[])::uuid[])",
    )
    .bind(vec![existing, fresh])
    .fetch_all(&mut *connection)
    .await
    .expect("both assigned defaults");
    assert_eq!(defaults, vec![true, true]);
    sqlx::query("UPDATE runner.runners SET durable_tasks_enabled = false WHERE id = $1::uuid")
        .bind(existing)
        .execute(&mut *connection)
        .await
        .expect("operator disables runner");
    drop(connection);
}
