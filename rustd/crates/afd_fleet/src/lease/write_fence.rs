//! A lease proved on the transaction of the write it guards.
//!
//! [`crate::lease::pull::Plane::standing`] proves a lease and lets go: right for
//! a read, and wrong for a write that lands afterwards, since a reclaim can take
//! the fleet in between. [`WriteFence`] proves the same lease again, on the
//! write's own transaction and under a shared lock on the lease row. A reclaim,
//! a renew and a settle each update that row, so each waits for the write to
//! commit, and an old holder resumed after a reclaim writes nothing.

use std::pin::Pin;

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_db::Precondition;
use sqlx::{PgConnection, Row as _};

use crate::lease::sql;
use crate::lease::sql::standing::SELECT_STANDING_SHARED;
use crate::lease::standing::fence_holds;

/// Where [`SELECT_STANDING_SHARED`] answers the lease's own token.
const COLUMN_FENCE: usize = 3;

/// Where it answers the fleet's live sequence.
const COLUMN_LIVE_SEQ: usize = 4;

/// `runner`'s live lease `lease`, still the fleet's holder under `presented`.
#[derive(Debug, Clone)]
pub struct WriteFence {
    /// The runner that presented the lease.
    runner: Uuid7,
    /// The lease it named.
    lease: Uuid7,
    /// The fencing token it presented.
    presented: u64,
    /// The instant the request is judged at.
    now: UnixMillis,
}

impl WriteFence {
    /// The fence a write by `runner` under `lease` and `presented` proves.
    #[must_use]
    pub const fn new(runner: Uuid7, lease: Uuid7, presented: u64, now: UnixMillis) -> Self {
        Self {
            runner,
            lease,
            presented,
            now,
        }
    }
}

impl Precondition for WriteFence {
    fn holds<'c>(
        &'c self,
        connection: &'c mut PgConnection,
    ) -> Pin<Box<dyn Future<Output = sqlx::Result<bool>> + Send + 'c>> {
        Box::pin(async move {
            let Some(row) = sqlx::query(SELECT_STANDING_SHARED)
                .bind(self.lease.as_str())
                .bind(self.runner.as_str())
                .bind(sql::LEASE_STATUS_ACTIVE)
                .bind(self.now.as_millis())
                .fetch_optional(connection)
                .await?
            else {
                return Ok(false);
            };
            Ok(fence_holds(
                row.try_get(COLUMN_FENCE)?,
                row.try_get(COLUMN_LIVE_SEQ)?,
                self.presented,
            ))
        })
    }
}
