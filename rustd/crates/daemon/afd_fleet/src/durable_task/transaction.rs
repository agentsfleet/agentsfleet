//! Cancellation closes the SQL socket before a blocked query can retain locks.

use afd_db::Db;
use scopeguard::ScopeGuard;
use sqlx::{Connection as _, PgConnection, Postgres, Transaction, pool::PoolConnection};

use crate::{Result, error::query};

pub(super) type Connection = ScopeGuard<PoolConnection<Postgres>, fn(PoolConnection<Postgres>)>;

pub(super) async fn acquire(database: &Db) -> Result<Connection> {
    Ok(scopeguard::guard(database.acquire().await?, discard))
}

pub(super) async fn begin(connection: &mut PgConnection) -> Result<Transaction<'_, Postgres>> {
    connection
        .begin()
        .await
        .map_err(query("begin bounded task transaction"))
}

pub(super) fn recycle(connection: Connection) {
    // Completion means no request is still executing. SQLx drains a queued
    // rollback before making this healthy connection available again.
    drop(ScopeGuard::into_inner(connection));
}

fn discard(connection: PoolConnection<Postgres>) {
    drop(connection.detach());
}
