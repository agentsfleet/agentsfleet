//! One daemon repository; callers prove authority on this connection's transaction.

use afd_core::id::Uuid7;
use sqlx::PgConnection;

use crate::{Checkpoint, CheckpointStore, Error, PortFuture, Result};

const LOAD: &str = include_str!("load.sql");
const INSERT: &str = include_str!("insert.sql");
const UPDATE: &str = include_str!("update.sql");

/// The erasable owner; account-wide or cross-host scopes cannot be constructed.
#[derive(Debug, Clone)]
pub enum Scope {
    /// One creator-private setup conversation.
    Draft(Uuid7),
    /// One branch of a fleet run under its live lease.
    Run {
        /// The live execution lease.
        lease: Uuid7,
        /// The erasable owning fleet.
        fleet: Uuid7,
        /// One root or child conversation branch.
        branch: Uuid7,
    },
}

impl Scope {
    /// Canonical identity assigned by the authorized host adapter.
    #[must_use]
    pub fn identity(&self) -> String {
        match self {
            Self::Draft(draft) => format!("draft:{draft}"),
            Self::Run {
                lease,
                fleet,
                branch,
            } => format!("run:{fleet}:{lease}:{branch}"),
        }
    }

    fn keys(&self) -> (Option<&str>, Option<&str>, Option<&str>, Option<&str>) {
        match self {
            Self::Draft(draft) => (Some(draft.as_str()), None, None, None),
            Self::Run {
                lease,
                fleet,
                branch,
            } => (
                None,
                Some(lease.as_str()),
                Some(fleet.as_str()),
                Some(branch.as_str()),
            ),
        }
    }
}

/// Borrow the caller's connection; no pool acquisition or authority check is hidden.
pub struct Repository<'a> {
    connection: &'a mut PgConnection,
    scope: Scope,
    id: Uuid7,
}

impl std::fmt::Debug for Repository<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Repository")
            .field("scope", &self.scope)
            .finish_non_exhaustive()
    }
}

impl<'a> Repository<'a> {
    /// `id` is a newly minted checkpoint row identity, used only on first insert.
    #[must_use]
    pub const fn new(connection: &'a mut PgConnection, scope: Scope, id: Uuid7) -> Self {
        Self {
            connection,
            scope,
            id,
        }
    }

    async fn read(&mut self) -> Result<Option<Checkpoint>> {
        let (draft, lease, fleet, branch) = self.scope.keys();
        let found: Option<sqlx::types::Json<Checkpoint>> = sqlx::query_scalar(LOAD)
            .bind(draft)
            .bind(lease)
            .bind(fleet)
            .bind(branch)
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
        checkpoint.validate_document()?;
        if checkpoint.identity.scope != self.scope.identity()
            || expected.checked_add(1) != Some(checkpoint.generation)
        {
            return Err(Error::stale());
        }
        let generation =
            i64::try_from(checkpoint.generation).map_err(|_overflow| Error::stale())?;
        let expected = i64::try_from(expected).map_err(|_overflow| Error::stale())?;
        let (draft, lease, fleet, branch) = self.scope.keys();
        let changed = sqlx::query(if expected == 0 { INSERT } else { UPDATE })
            .bind(self.id.as_str())
            .bind(draft)
            .bind(lease)
            .bind(fleet)
            .bind(branch)
            .bind(generation)
            .bind(sqlx::types::Json(&checkpoint))
            .bind(expected)
            .execute(&mut *self.connection)
            .await
            .map_err(Error::boundary)?;
        if changed.rows_affected() == 1 || self.read().await?.as_ref() == Some(&checkpoint) {
            Ok(())
        } else {
            Err(Error::stale())
        }
    }
}

impl CheckpointStore for Repository<'_> {
    fn load(&mut self) -> PortFuture<'_, Option<Checkpoint>> {
        Box::pin(self.read())
    }
    fn commit(&mut self, expected_generation: u64, checkpoint: Checkpoint) -> PortFuture<'_, ()> {
        Box::pin(self.publish(expected_generation, checkpoint))
    }
}
