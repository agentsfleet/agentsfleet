//! A fleet chat's context checkpoint store, one row per thread, whose id is
//! the fleet's. Callers prove authority on this connection's transaction.
//!
//! A [`Repository::fenced`] save proves it in the statement instead: it writes
//! only while the run it names holds the thread at its fence, so a run a
//! reassignment superseded writes nothing and is refused as stale-fenced.

use af_core::id::Uuid7;
use sqlx::PgConnection;

use af_context::{Checkpoint, CheckpointStore, Error, PortFuture, Result};

const LOAD: &str = include_str!("load.sql");
const FENCED_INSERT: &str = include_str!("fenced_insert.sql");
const FENCED_UPDATE: &str = include_str!("fenced_update.sql");
const HOLDS: &str = include_str!("holds.sql");

/// The identity a fleet chat's checkpoint carries: one erasable owner, the
/// fleet, so no account-wide or cross-host scope can be constructed.
#[must_use]
pub fn fleet_identity(fleet: &Uuid7) -> String {
    format!("fleet:{fleet}")
}

/// The run a fenced save names. The save writes only while `run` is
/// `runner`'s run of the thread, at `fence`, with `active` its status, and
/// `fence` is still the thread's.
///
/// `active` is the daemon's live run status, which this crate does not own.
#[derive(Debug, Clone, Copy)]
pub struct Holder<'h> {
    /// The run saving.
    pub run: &'h Uuid7,
    /// The runner the run was assigned to.
    pub runner: &'h Uuid7,
    /// The fencing token the run holds the thread at.
    pub fence: u64,
    /// The status a live run holds.
    pub active: &'h str,
}

/// `fence` as the stored `bigint`. A fence past it names no run, so it is
/// refused as one that no longer holds the thread.
///
/// # Errors
/// The stale-fence refusal, for a fence no thread can hold.
pub fn stored_fence(fence: u64) -> Result<i64> {
    i64::try_from(fence).map_err(|_overflow| Error::stale_fence())
}

/// Borrow the caller's connection; no pool acquisition or authority check is hidden.
pub struct Repository<'a> {
    connection: &'a mut PgConnection,
    /// The fleet whose thread the checkpoint keeps: `thread.threads.id`.
    fleet: Uuid7,
    id: Uuid7,
    holder: Option<Holder<'a>>,
}

impl std::fmt::Debug for Repository<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Repository")
            .field("fleet", &self.fleet)
            .field("holder", &self.holder)
            .finish_non_exhaustive()
    }
}

impl<'a> Repository<'a> {
    /// `fleet`'s store, which reads only: a save through it is refused as
    /// stale-fenced. `id` is a newly minted checkpoint row identity, used
    /// only on first insert.
    #[must_use]
    pub const fn new(connection: &'a mut PgConnection, fleet: Uuid7, id: Uuid7) -> Self {
        Self {
            connection,
            fleet,
            id,
            holder: None,
        }
    }

    /// `fleet`'s store whose saves write only while `holder` holds the
    /// thread at its fence; `id` as for [`Repository::new`].
    #[must_use]
    pub const fn fenced(
        connection: &'a mut PgConnection,
        fleet: Uuid7,
        id: Uuid7,
        holder: Holder<'a>,
    ) -> Self {
        Self {
            connection,
            fleet,
            id,
            holder: Some(holder),
        }
    }

    /// The one checkpoint the thread owns, validated, or none before the
    /// first publication.
    ///
    /// # Errors
    /// The store would not answer, or the stored document is not a valid
    /// checkpoint.
    pub async fn load(&mut self) -> Result<Option<Checkpoint>> {
        let found: Option<sqlx::types::Json<Checkpoint>> = sqlx::query_scalar(LOAD)
            .bind(&self.fleet)
            .fetch_optional(&mut *self.connection)
            .await
            .map_err(Error::boundary)?;
        let checkpoint = found.map(|value| value.0);
        if let Some(value) = &checkpoint {
            value.validate_document()?;
        }
        Ok(checkpoint)
    }

    async fn publish(&mut self, expected: u64, checkpoint: Checkpoint) -> Result<()> {
        // Every save names the run that holds the thread; a store opened with
        // [`Repository::new`] holds none, so it only reads.
        let Some(holder) = self.holder else {
            return Err(Error::stale_fence());
        };
        checkpoint.validate_document()?;
        if checkpoint.identity.scope != fleet_identity(&self.fleet)
            || expected.checked_add(1) != Some(checkpoint.generation)
        {
            return Err(Error::stale());
        }
        let generation =
            i64::try_from(checkpoint.generation).map_err(|_overflow| Error::stale())?;
        let expected = i64::try_from(expected).map_err(|_overflow| Error::stale())?;
        let fence = stored_fence(holder.fence)?;
        // The insert names the new row's id; the update finds the row by thread.
        let statement = if expected == 0 {
            sqlx::query(FENCED_INSERT).bind(&self.id)
        } else {
            sqlx::query(FENCED_UPDATE)
        };
        let changed = statement
            .bind(&self.fleet)
            .bind(generation)
            .bind(sqlx::types::Json(&checkpoint))
            .bind(expected)
            .bind(holder.run)
            .bind(holder.runner)
            .bind(fence)
            .bind(holder.active)
            .execute(&mut *self.connection)
            .await
            .map_err(Error::boundary)?;
        if changed.rows_affected() == 1 {
            return Ok(());
        }
        // A run that no longer holds the thread is refused as such, even when
        // a save of the same checkpoint already landed while it held it.
        if !self.holds(holder, fence).await? {
            return Err(Error::stale_fence());
        }
        if self.load().await?.as_ref() == Some(&checkpoint) {
            Ok(())
        } else {
            Err(Error::stale())
        }
    }

    /// Whether `holder` still holds the thread at `fence`.
    async fn holds(&mut self, holder: Holder<'_>, fence: i64) -> Result<bool> {
        sqlx::query_scalar(HOLDS)
            .bind(&self.fleet)
            .bind(holder.run)
            .bind(holder.runner)
            .bind(fence)
            .bind(holder.active)
            .fetch_one(&mut *self.connection)
            .await
            .map_err(Error::boundary)
    }
}

impl CheckpointStore for Repository<'_> {
    fn commit(&mut self, expected_generation: u64, checkpoint: Checkpoint) -> PortFuture<'_, ()> {
        Box::pin(self.publish(expected_generation, checkpoint))
    }
}
