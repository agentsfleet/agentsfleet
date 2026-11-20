//! A run whose assignment committed but whose frame never left the daemon
//! goes back to its queue at once.
//!
//! The gateway writes a run before it charges, records and hands over its
//! frame. When one of those fails, the runner never heard of the run, and
//! left alone it would hold its thread until its lease lapsed, then be
//! billed the lease as runtime. Released here, the run ends expired with
//! nothing to bill, its thread is freed, and the next assignment, on any
//! runner, takes the same work at the next fence. A frame too large ever to
//! send is not released: the gateway fails that run, since a retry would fail
//! the same way.

use af_core::id::Uuid7;

use crate::error::{Result, query};
use crate::run::sql;
use crate::run::store::Runs;

/// The statement that releases one unsent run, fenced. See its header.
const RELEASE_UNSENT: &str = include_str!("release.sql");

/// Statement name, for the context a query failure carries.
const CONTEXT_RELEASE_UNSENT: &str = "release an unsent run";

impl Runs {
    /// Releases `run_id`, which `runner_id` was assigned at `fencing_token`
    /// and never sent, then wakes its thread so the work is assigned again.
    ///
    /// Fenced: only that runner's run at that fence, still active, under a
    /// thread still at that fence, is released. Anything else is untouched
    /// and answers `false`.
    ///
    /// # Errors
    /// Reports a datastore that would not answer; the run then lapses at its
    /// expiry, as it would have without this.
    pub async fn release_unsent(
        &self,
        run_id: &Uuid7,
        runner_id: &Uuid7,
        fencing_token: u64,
    ) -> Result<bool> {
        // A token past `i64` names no row, so it releases nothing.
        let Ok(fence) = i64::try_from(fencing_token) else {
            return Ok(false);
        };
        let mut connection = self.pool().acquire().await?;
        let freed: Option<Uuid7> = sqlx::query_scalar(RELEASE_UNSENT)
            .bind(run_id)
            .bind(runner_id)
            .bind(fence)
            .bind(sql::RUN_STATUS_ACTIVE)
            .bind(sql::RUN_STATUS_EXPIRED)
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_RELEASE_UNSENT))?;
        drop(connection);
        let Some(thread) = freed else {
            return Ok(false);
        };
        afd_bus::wake::publish(&self.streams(), thread).await;
        Ok(true)
    }
}
