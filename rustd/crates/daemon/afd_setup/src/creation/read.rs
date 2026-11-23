//! Shared fleet reads retain setup progress without exposing any private conversation.

use super::{
    CONTEXT, Creations, SetupProgress, etag, progress, sql,
    stored::{Stored, StoredRow},
};
use crate::{
    Access, Result, access,
    error::{ErrorKind, raise::query},
    review,
    types::{Review, StartOutcome},
};
use af_core::id::Uuid7;
use af_fleet_runtime::Definition;
use jiff::Timestamp;
use serde::Serialize;
use sqlx::{Connection as _, PgConnection};

/// Native fleet snapshot used by browser and terminal adapters.
#[derive(Debug, Serialize)]
pub struct SavedFleet {
    /// Shared runtime identity, unrelated to private draft ownership.
    pub id: Uuid7,
    /// Instance slug.
    pub name: String,
    /// Existing fleet lifecycle vocabulary.
    pub status: String,
    /// Name/status version.
    pub etag: String,
    /// Definition-only version.
    pub setup_etag: String,
    /// Decimal setup revision.
    pub setup_revision: String,
    /// Exact immutable setup identity for explicit adoption by existing work.
    pub setup_revision_id: Uuid7,
    /// Policy interpretation of this setup.
    pub policy_semantics: String,
    /// Shared immutable setup, without conversation or credentials.
    pub definition: Definition,
    /// Actual current installation checks.
    pub setup: SetupProgress,
    /// Initial turn outcome, independent of completed work.
    pub start: StartOutcome,
    /// Creation time.
    #[serde(with = "af_wire::instant")]
    pub created_at: Timestamp,
    /// Latest resource mutation.
    #[serde(with = "af_wire::instant")]
    pub updated_at: Timestamp,
    /// Exact activity count.
    pub events_processed: String,
    /// Exact billed nanos.
    pub budget_used_nanos: String,
    /// Current unanswered approval gates.
    pub pending_approvals: i64,
    /// Current independent shared-memory read permission.
    pub memory_reads_workspace: bool,
    /// Current independent shared-memory publication permission.
    pub memory_publishes_workspace: bool,
}

impl Creations {
    /// Reads current shared setup and live prerequisites without advancing it,
    /// locking no fleet, person, install or grant: a read that held the fleet
    /// row would stall the fleet's intake and every report for its length.
    ///
    /// Read committed rather than on one snapshot, for the reason
    /// [`crate::Setup::read_draft`] gives: the credential inspection opens its
    /// secrets `FOR SHARE`.
    ///
    /// # Errors
    /// Refuses missing fleets and current access loss; preserves store failures.
    pub async fn read(&self, access: Access<'_>, fleet: &Uuid7) -> Result<SavedFleet> {
        let (row, checked) = self.snapshot(access, fleet).await?;
        Ok(project(row, &checked))
    }

    /// The fleet and its current review, read without a lock. Setup's own
    /// steps read it too: a lock taken here ended at this commit, before any
    /// step acted on what it read, so it protected nothing. Each write
    /// rechecks under its own lock (`activate.rs`, `start/recover.rs`).
    pub(super) async fn snapshot(
        &self,
        access: Access<'_>,
        fleet: &Uuid7,
    ) -> Result<(Stored, Review)> {
        let mut connection = self.setup.drafts.database.acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        access::unlocked(&mut tx, access).await?;
        let row = stored(&mut tx, access.workspace, fleet).await?;
        let checked = self.review(&mut tx, &row, review::Hold::Nothing).await?;
        tx.commit().await.map_err(query(CONTEXT))?;
        Ok((row, checked))
    }

    pub(super) async fn review(
        &self,
        connection: &mut PgConnection,
        row: &Stored,
        hold: review::Hold,
    ) -> Result<Review> {
        let mut context = row.accepted.context.clone();
        context.linked_fleet_id = Some(row.id.clone());
        review::build(
            &self.setup,
            connection,
            &context,
            &row.accepted.candidate(row.definition.clone()),
            af_core::clock::now(),
            hold,
        )
        .await
    }
}

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

/// The fleet with its head revision, counters and open gates, as the caller's
/// statement sees it: under [`lock`]'s row lock for a write, and unlocked for
/// a read.
async fn stored(connection: &mut PgConnection, workspace: &Uuid7, fleet: &Uuid7) -> Result<Stored> {
    let row: Option<StoredRow> = sqlx::query_as(sql::LOCKED)
        .bind(fleet)
        .bind(workspace)
        .fetch_optional(connection)
        .await
        .map_err(query(CONTEXT))?;
    row.ok_or(ErrorKind::FleetMissing)?.decode()
}

fn project(row: Stored, review: &Review) -> SavedFleet {
    let setup = progress::current(&row, review);
    SavedFleet {
        etag: etag(&row.id, row.resource_version),
        setup_etag: etag(&row.id, row.head_revision),
        setup_revision: row.head_revision.to_string(),
        setup_revision_id: row.head_revision_id,
        policy_semantics: row.accepted.policy_semantics,
        id: row.id,
        name: row.name,
        status: row.status,
        definition: row.definition,
        setup,
        start: row.initial_start.outcome,
        created_at: row.created_at,
        updated_at: row.updated_at,
        events_processed: row.events_processed.to_string(),
        budget_used_nanos: row.budget_used_nanos.to_string(),
        pending_approvals: row.pending_approvals,
        memory_reads_workspace: row.memory_reads_workspace,
        memory_publishes_workspace: row.memory_publishes_workspace,
    }
}
