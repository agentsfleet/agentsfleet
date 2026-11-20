//! A run proved on the transaction of the write it guards.
//!
//! [`crate::run::pull::RunService::standing`] proves a run and lets go: right for
//! a read, and wrong for a write that lands afterwards, since a reassignment can take
//! the fleet in between. [`WriteFence`] proves the same run again, on the
//! write's own transaction and under a shared lock on the run row. A reassignment,
//! a renew and a settle each update that row, so each waits for the write to
//! commit, and an old holder resumed after a reassignment writes nothing.

use std::pin::Pin;

use af_core::id::Uuid7;
use afd_db::Precondition;
use sqlx::PgConnection;

use crate::run::sql;
use crate::run::sql::standing::SELECT_STANDING_SHARED;
use crate::run::standing::LiveRun;

/// `runner`'s live run `run`, still the fleet's holder under `presented`.
#[derive(Debug, Clone)]
pub struct WriteFence {
    /// The runner that presented the run.
    runner: Uuid7,
    /// The run it named.
    run: Uuid7,
    /// The fencing token it presented.
    presented: u64,
}

impl WriteFence {
    /// The fence a write by `runner` under `run` and `presented` proves.
    #[must_use]
    pub const fn new(runner: Uuid7, run: Uuid7, presented: u64) -> Self {
        Self {
            runner,
            run,
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
            let found: Option<LiveRun> = sqlx::query_as(SELECT_STANDING_SHARED)
                .bind(&self.run)
                .bind(&self.runner)
                .bind(sql::RUN_STATUS_ACTIVE)
                .fetch_optional(connection)
                .await?;
            Ok(found.is_some_and(|run| run.holds(self.presented)))
        })
    }
}
