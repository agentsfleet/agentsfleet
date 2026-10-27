//! Which workspace a fleet is in, and the two shared-memory grants it holds.
//!
//! Read and written on `fleet.fleets` as the api role, BEFORE any store takes
//! the memory role — `memory_runtime` cannot see `core`, and whichever store
//! holds the rows, the grants are this control plane's to enforce.

use std::collections::HashMap;

use afd_api_wire::fleet::{MemoryAccess, MemoryAccessRequest};
use afd_core::id::Uuid7;
use afd_db::Db;

use crate::error::{Result, query};

/// Statement names, for the context a query failure carries.
const CONTEXT_ACCESS: &str = "memory fleet access";
const CONTEXT_GRANT: &str = "memory access change";
const CONTEXT_NAMES: &str = "memory writer names";

/// A fleet's workspace and grants. `$1` fleet.
const SELECT_ACCESS: &str = include_str!("sql/select_access.sql");

/// Sets either grant, keeping the other when its parameter is NULL, and only
/// on a fleet the named workspace holds.
///
/// `$1` fleet, `$2` workspace, `$3` read, `$4` publish.
const UPDATE_ACCESS: &str = include_str!("sql/update_access.sql");
/// Workspace-scoped grant lock; fleet and workspace.
const LOCK_ACCESS: &str = include_str!("sql/lock_access.sql");

/// Every fleet's name in a workspace, for naming a shared entry's writer.
/// `$1` workspace.
const SELECT_FLEET_NAMES: &str = include_str!("sql/select_fleet_names.sql");

/// A fleet's two grants, as the statements return them.
#[derive(Debug, sqlx::FromRow)]
struct GrantsRow {
    memory_reads_workspace: bool,
    memory_publishes_workspace: bool,
}

#[derive(Debug, sqlx::FromRow)]
struct LockedGrants {
    #[sqlx(flatten)]
    grants: GrantsRow,
    activity_id: String,
}

impl From<GrantsRow> for MemoryAccess {
    fn from(row: GrantsRow) -> Self {
        Self {
            read: row.memory_reads_workspace,
            publish: row.memory_publishes_workspace,
        }
    }
}

/// A fleet's workspace beside its grants.
#[derive(Debug, sqlx::FromRow)]
struct AccessRow {
    workspace_id: String,
    #[sqlx(flatten)]
    grants: GrantsRow,
}

/// Where a fleet is and what it may do with the workspace's shared memory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Access {
    /// The workspace the fleet belongs to.
    pub(crate) workspace: Uuid7,
    /// Its two grants.
    pub(crate) grants: MemoryAccess,
}

/// The grant reads and writes, over the api-role pool.
#[derive(Debug, Clone)]
pub(crate) struct Directory {
    database: Db,
}

impl Directory {
    /// A directory reading `fleet.fleets` through `database`.
    pub(crate) const fn new(database: Db) -> Self {
        Self { database }
    }

    /// `fleet`'s workspace and grants, or `None` for a fleet that does not exist.
    pub(crate) async fn of(&self, fleet: &Uuid7) -> Result<Option<Access>> {
        let mut connection = self.database.acquire().await?;
        let Some(row) = sqlx::query_as::<_, AccessRow>(SELECT_ACCESS)
            .bind(fleet.as_str())
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_ACCESS))?
        else {
            return Ok(None);
        };
        Ok(Some(Access {
            workspace: Uuid7::parse(&row.workspace_id)?,
            grants: row.grants.into(),
        }))
    }

    /// Applies `change` to `fleet` in `workspace`, answering both grants as
    /// they now stand, or `None` when the workspace holds no such fleet.
    pub(crate) async fn set(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
        change: MemoryAccessRequest,
        streams: Option<&afd_dragonfly::FleetStreams>,
    ) -> Result<Option<MemoryAccess>> {
        let mut tx = sqlx::Transaction::begin(self.database.acquire().await?, None)
            .await
            .map_err(query(CONTEXT_GRANT))?;
        let Some(previous) = sqlx::query_as::<_, LockedGrants>(LOCK_ACCESS)
            .bind(fleet.as_str())
            .bind(workspace.as_str())
            .fetch_optional(&mut *tx)
            .await
            .map_err(query(CONTEXT_GRANT))?
        else {
            return Ok(None);
        };
        let changed = change
            .read
            .is_some_and(|value| value != previous.grants.memory_reads_workspace)
            || change
                .publish
                .is_some_and(|value| value != previous.grants.memory_publishes_workspace);
        if !changed {
            return Ok(Some(previous.grants.into()));
        }
        let access = sqlx::query_as::<_, GrantsRow>(UPDATE_ACCESS)
            .bind(fleet.as_str())
            .bind(workspace.as_str())
            .bind(change.read)
            .bind(change.publish)
            .fetch_one(&mut *tx)
            .await
            .map_err(query(CONTEXT_GRANT))?;
        let announcement = afd_events::activity::record_on(
            &mut tx,
            workspace,
            fleet,
            &previous.activity_id,
            afd_events::activity::Activity::MemorySharingChanged {
                read: access.memory_reads_workspace,
                publish: access.memory_publishes_workspace,
            },
        )
        .await
        .map_err(query(CONTEXT_GRANT))?;
        afd_events::activity::commit(tx, announcement, streams)
            .await
            .map_err(query(CONTEXT_GRANT))?;
        Ok(Some(access.into()))
    }

    /// Every fleet's name in `workspace`, keyed by its identifier.
    pub(crate) async fn names(&self, workspace: &Uuid7) -> Result<HashMap<String, String>> {
        let mut connection = self.database.acquire().await?;
        Ok(sqlx::query_as::<_, (String, String)>(SELECT_FLEET_NAMES)
            .bind(workspace.as_str())
            .fetch_all(&mut *connection)
            .await
            .map_err(query(CONTEXT_NAMES))?
            .into_iter()
            .collect())
    }
}
