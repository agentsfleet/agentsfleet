//! A fleet as a member of its workspace reads it.

use af_core::id::Uuid7;
use af_fleet_runtime::config::HttpEgress;
use jiff::Timestamp;
use jiff_sqlx::Timestamp as StoredInstant;
use sqlx::{Connection as _, PgConnection};

use super::{CONTEXT, Creations, etag, sql};
use crate::{
    Access, Result, access,
    error::{
        ErrorKind,
        raise::{absent, query},
    },
    trigger,
    types::SavedFleet,
};

/// [`sql::LOCKED`]'s row, by column name.
#[derive(Debug, sqlx::FromRow)]
pub(super) struct Stored {
    pub(super) id: Uuid7,
    pub(super) name: String,
    pub(super) status: String,
    pub(super) model_ref: String,
    pub(super) http_egress: String,
    pub(super) package_domains: bool,
    pub(super) resource_version: i64,
    #[sqlx(try_from = "StoredInstant")]
    created_at: Timestamp,
    #[sqlx(try_from = "StoredInstant")]
    updated_at: Timestamp,
    memory_reads_workspace: bool,
    memory_publishes_workspace: bool,
    events_processed: i64,
    budget_used_nanos: i64,
    pending_approvals: i64,
}

impl Stored {
    /// The row's HTTP egress scope; a spelling this build cannot read is a
    /// stored fault.
    pub(super) fn egress(&self) -> Result<HttpEgress> {
        HttpEgress::parse(&self.http_egress).ok_or_else(|| absent("http_egress"))
    }
}

impl Creations {
    /// Reads `fleet` without a lock: a read that held the fleet row would
    /// stall its intake and every report for its length.
    ///
    /// # Errors
    /// A caller who is not a current member, a fleet outside the workspace,
    /// and datastore failures.
    pub async fn read(&self, access: Access<'_>, fleet: &Uuid7) -> Result<SavedFleet> {
        let (row, triggers) = {
            let mut connection = self.database.acquire().await?;
            let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
            access::unlocked(&mut tx, access).await?;
            let row = stored(&mut tx, access.workspace, fleet).await?;
            let triggers = trigger::list(&mut tx, fleet).await?;
            tx.commit().await.map_err(query(CONTEXT))?;
            (row, triggers)
        };
        // Read after the fleet's connection went back: the grant store takes
        // its own, and a request holds one at a time.
        let grants = self
            .grants
            .page(access.workspace, fleet)
            .await?
            .ok_or(ErrorKind::FleetMissing)?;
        project(row, triggers, grants)
    }
}

/// Locks `fleet`'s row for a write, then reads it under that lock.
pub(super) async fn lock(
    connection: &mut PgConnection,
    workspace: &Uuid7,
    fleet: &Uuid7,
) -> Result<Stored> {
    let locked: Option<Uuid7> = sqlx::query_scalar(sql::LOCK)
        .bind(fleet)
        .bind(workspace)
        .fetch_optional(&mut *connection)
        .await
        .map_err(query(CONTEXT))?;
    locked.ok_or(ErrorKind::FleetMissing)?;
    stored(connection, workspace, fleet).await
}

/// The fleet with its counters and open gates, as the caller's statement sees it.
pub(super) async fn stored(
    connection: &mut PgConnection,
    workspace: &Uuid7,
    fleet: &Uuid7,
) -> Result<Stored> {
    let row: Option<Stored> = sqlx::query_as(sql::LOCKED)
        .bind(fleet)
        .bind(workspace)
        .fetch_optional(connection)
        .await
        .map_err(query(CONTEXT))?;
    row.ok_or_else(|| ErrorKind::FleetMissing.into())
}

/// The read a create, a read and a change answer with.
pub(super) fn project(
    row: Stored,
    triggers: Vec<trigger::TriggerRow>,
    grants: Vec<afd_approval::GrantRow>,
) -> Result<SavedFleet> {
    let http_egress = match row.egress()? {
        HttpEgress::Restricted => af_wire::policy::HttpEgress::Restricted,
        HttpEgress::Public => af_wire::policy::HttpEgress::Public,
    };
    Ok(SavedFleet {
        etag: etag(&row.id, row.resource_version),
        id: row.id,
        name: row.name,
        status: row.status,
        model_ref: row.model_ref,
        http_egress,
        package_domains: row.package_domains,
        triggers,
        grants,
        memory_reads_workspace: row.memory_reads_workspace,
        memory_publishes_workspace: row.memory_publishes_workspace,
        created_at: row.created_at,
        updated_at: row.updated_at,
        events_processed: row.events_processed.to_string(),
        budget_used_nanos: row.budget_used_nanos.to_string(),
        pending_approvals: row.pending_approvals,
    })
}
