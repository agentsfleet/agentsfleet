//! The database one §7 scenario runs against.
//!
//! Split from `e2e.rs` by concern rather than by size (RULE FLL): this is the
//! only place that answers where a scenario's daemon points.
//!
//! # The lane's database, not one per scenario
//!
//! Each scenario used to `CREATE DATABASE`, migrate all forty-seven
//! `schema/*.sql` files into it, boot a daemon against it, and `DROP … WITH
//! (FORCE)` afterwards. It now boots against the database the lane already
//! migrated — see [`afd_db::test_util`] on what the per-test database cost and
//! what it was actually buying.
//!
//! What keeps scenarios apart is `e2e::unique_ids`: every fleet, workspace and
//! tenant a scenario touches is minted for it, and the daemon's own statements
//! carry those in their predicates. The one seed that writes a GLOBAL row —
//! the model-library rate — is an upsert on `(provider, model_id)`, so two
//! scenarios agreeing on a price is not a collision.
#![allow(
    dead_code,
    reason = "test support: shared by several test binaries, each using a subset"
)]
#![expect(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]

use sqlx::AssertSqlSafe;

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

/// The URL a scenario's daemon boots against — the lane's own, already migrated.
///
/// A function rather than the caller reading the knob, because the migration
/// used to happen here and a reader following that thread should land on the
/// note above rather than on nothing.
pub(crate) fn scenario_database(base_url: &str) -> String {
    base_url.to_owned()
}
