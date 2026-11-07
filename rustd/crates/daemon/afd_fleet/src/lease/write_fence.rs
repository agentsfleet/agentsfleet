//! A lease proved on the transaction of the write it guards.
//!
//! [`crate::lease::pull::Plane::standing`] proves a lease and lets go: right for
//! a read, and wrong for a write that lands afterwards, since a reclaim can take
//! the fleet in between. [`WriteFence`] proves the same lease again, on the
//! write's own transaction and under a shared lock on the lease row. A reclaim,
//! a renew and a settle each update that row, so each waits for the write to
//! commit, and an old holder resumed after a reclaim writes nothing.

use std::pin::Pin;

use afd_core::id::Uuid7;
use afd_db::Precondition;
use sqlx::PgConnection;

use crate::lease::sql;
use crate::lease::sql::standing::SELECT_STANDING_SHARED;
use crate::lease::standing::LiveLease;

/// `runner`'s live lease `lease`, still the fleet's holder under `presented`.
#[derive(Debug, Clone)]
pub struct WriteFence {
    /// The runner that presented the lease.
    runner: Uuid7,
    /// The lease it named.
    lease: Uuid7,
    /// The fencing token it presented.
    presented: u64,
}

impl WriteFence {
    /// The fence a write by `runner` under `lease` and `presented` proves.
    #[must_use]
    pub const fn new(runner: Uuid7, lease: Uuid7, presented: u64) -> Self {
        Self {
            runner,
            lease,
            presented,
        }
    }
}

impl Precondition for WriteFence {
    fn holds<'c>(
        &'c self,
        connection: &'c mut PgConnection,
    ) -> Pin<Box<dyn Future<Output = sqlx::Result<bool>> + Send + 'c>> {
        Box::pin(async move {
            let found: Option<LiveLease> = sqlx::query_as(SELECT_STANDING_SHARED)
                .bind(self.lease.as_str())
                .bind(self.runner.as_str())
                .bind(sql::LEASE_STATUS_ACTIVE)
                .fetch_optional(connection)
                .await?;
            Ok(found.is_some_and(|lease| lease.holds(self.presented)))
        })
    }
}
