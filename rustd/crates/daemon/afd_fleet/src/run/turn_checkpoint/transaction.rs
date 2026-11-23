//! A cancelled mutation closes its SQL socket before blocked work can retain locks.

use afd_db::Db;
use scopeguard::ScopeGuard;
use sqlx::{Postgres, pool::PoolConnection};

use crate::Result;

pub(super) type Connection = ScopeGuard<PoolConnection<Postgres>, fn(PoolConnection<Postgres>)>;

pub(super) async fn acquire(database: &Db) -> Result<Connection> {
    Ok(scopeguard::guard(database.acquire().await?, discard))
}

pub(super) fn recycle(connection: Connection) {
    drop(ScopeGuard::into_inner(connection));
}

fn discard(connection: PoolConnection<Postgres>) {
    drop(connection.detach());
}
