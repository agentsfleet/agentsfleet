//! Which workspace a fleet is in, and the two shared-memory grants it holds.
//!
//! Read and written on `core.fleets` as the api role, BEFORE any store takes
//! the memory role — `memory_runtime` cannot see `core`, and whichever store
//! holds the rows, the grants are this control plane's to enforce.

use std::collections::HashMap;

use afd_core::id::Uuid7;
use afd_db::Db;
use afd_wire::fleet::{MemoryAccess, MemoryAccessRequest};
use sqlx::Row as _;

use crate::error::{Result, query};

/// Statement names, for the context a query failure carries.
const CONTEXT_ACCESS: &str = "memory fleet access";
const CONTEXT_GRANT: &str = "memory access change";
const CONTEXT_NAMES: &str = "memory writer names";

/// A fleet's workspace and grants. `$1` fleet.
const SELECT_ACCESS: &str = "\
SELECT workspace_id::text, memory_reads_workspace, memory_publishes_workspace
FROM core.fleets WHERE id = $1::uuid";

/// Sets either grant, keeping the other when its parameter is NULL, and only
/// on a fleet the named workspace holds.
///
/// `$1` fleet, `$2` workspace, `$3` read, `$4` publish.
const UPDATE_ACCESS: &str = "\
UPDATE core.fleets
SET memory_reads_workspace = COALESCE($3, memory_reads_workspace),
    memory_publishes_workspace = COALESCE($4, memory_publishes_workspace)
WHERE id = $1::uuid AND workspace_id = $2::uuid
RETURNING memory_reads_workspace, memory_publishes_workspace";

/// Every fleet's name in a workspace, for naming a shared entry's writer.
/// `$1` workspace.
const SELECT_FLEET_NAMES: &str = "\
SELECT id::text, name FROM core.fleets WHERE workspace_id = $1::uuid";

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
    /// A directory reading `core.fleets` through `database`.
    pub(crate) const fn new(database: Db) -> Self {
        Self { database }
    }

    /// `fleet`'s workspace and grants, or `None` for a fleet that does not exist.
    pub(crate) async fn of(&self, fleet: &Uuid7) -> Result<Option<Access>> {
        let mut connection = self.database.acquire().await?;
        let Some(row) = sqlx::query(SELECT_ACCESS)
            .bind(fleet.as_str())
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_ACCESS))?
        else {
            return Ok(None);
        };
        let workspace: String = row.try_get(0).map_err(query(CONTEXT_ACCESS))?;
        Ok(Some(Access {
            workspace: Uuid7::parse(&workspace)?,
            grants: MemoryAccess {
                read: row.try_get(1).map_err(query(CONTEXT_ACCESS))?,
                publish: row.try_get(2).map_err(query(CONTEXT_ACCESS))?,
            },
        }))
    }

    /// Applies `change` to `fleet` in `workspace`, answering both grants as
    /// they now stand, or `None` when the workspace holds no such fleet.
    pub(crate) async fn set(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
        change: MemoryAccessRequest,
    ) -> Result<Option<MemoryAccess>> {
        let mut connection = self.database.acquire().await?;
        sqlx::query(UPDATE_ACCESS)
            .bind(fleet.as_str())
            .bind(workspace.as_str())
            .bind(change.read)
            .bind(change.publish)
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_GRANT))?
            .map(|row| {
                Ok(MemoryAccess {
                    read: row.try_get(0).map_err(query(CONTEXT_GRANT))?,
                    publish: row.try_get(1).map_err(query(CONTEXT_GRANT))?,
                })
            })
            .transpose()
    }

    /// Every fleet's name in `workspace`, keyed by its identifier.
    pub(crate) async fn names(&self, workspace: &Uuid7) -> Result<HashMap<String, String>> {
        let mut connection = self.database.acquire().await?;
        sqlx::query(SELECT_FLEET_NAMES)
            .bind(workspace.as_str())
            .fetch_all(&mut *connection)
            .await
            .map_err(query(CONTEXT_NAMES))?
            .iter()
            .map(|row| {
                Ok((
                    row.try_get(0).map_err(query(CONTEXT_NAMES))?,
                    row.try_get(1).map_err(query(CONTEXT_NAMES))?,
                ))
            })
            .collect()
    }
}
