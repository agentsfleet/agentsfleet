//! Creating and dropping the database one §7 scenario runs against.
//!
//! Split from `e2e.rs` by concern rather than by size (RULE FLL): this is the
//! only place that talks to the lane's ADMIN database, and everything it does
//! happens either side of the daemon's whole life — a `CREATE DATABASE` before
//! `boot` and a `DROP … WITH (FORCE)` after the pools close.
//!
//! Migrating here rather than leaving it to the daemon is deliberate: `boot`
//! connects with the api role and does not migrate, so a daemon pointed at an
//! empty database fails its first query instead of its boot, and the failure
//! reads as a broken port rather than a missing schema.
#![allow(
    dead_code,
    reason = "test support: shared by several test binaries, each using a subset"
)]
#![expect(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]

use afd_core::env::MapEnv;
use afd_db::Migrator;
use afd_db::config::DbRole;
use sqlx::AssertSqlSafe;

/// The lane's admin URL with its database path replaced.
pub(crate) fn database_url(base_url: &str, name: &str) -> String {
    let (prefix, tail) = base_url
        .rsplit_once('/')
        .expect("a Postgres URL carries a database path");
    let query = tail.split_once('?').map_or("", |(_, query)| query);
    if query.is_empty() {
        format!("{prefix}/{name}")
    } else {
        format!("{prefix}/{name}?{query}")
    }
}

/// Runs one statement on the lane's admin database.
pub(crate) async fn admin(base_url: &str, statement: AssertSqlSafe<String>) {
    let pool = sqlx::PgPool::connect(base_url)
        .await
        .expect("the lane's database must be reachable");
    let mut connection = pool.acquire().await.expect("an admin connection");
    sqlx::query(statement)
        .execute(&mut *connection)
        .await
        .expect("the admin statement must run");
    drop(connection);
    pool.close().await;
}

/// Creates and migrates a database for one scenario, and answers its URL.
pub(crate) async fn fresh_database(base_url: &str, name: &str) -> String {
    // The name is a process id and a counter, never input — which is what makes
    // interpolating it safe, since Postgres does not bind identifiers.
    admin(base_url, AssertSqlSafe(format!("CREATE DATABASE {name}"))).await;
    let url = database_url(base_url, name);

    // Migrated HERE rather than left to the daemon: `boot` connects with the
    // api role and does not migrate, so a daemon pointed at an empty database
    // fails its first query instead of its boot, and the failure reads as a
    // broken port rather than a missing schema.
    let env = MapEnv::from_pairs(
        DbRole::ALL
            .iter()
            .map(|each| (each.url_knob(), url.as_str())),
    );
    let migrator = afd_db::Db::connect(
        &afd_db::config::PoolConfig::resolve(&env, DbRole::Migrator)
            .expect("the scenario URL resolves"),
    )
    .await
    .expect("the scenario database must accept a connection");
    Migrator::new()
        .run(&migrator)
        .await
        .expect("the schema must apply to a fresh database");
    drop(migrator);
    url
}
