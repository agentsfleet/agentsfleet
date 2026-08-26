//! Which integrations a fleet may mint against.
//!
//! Separate from [`super::store`] because it reads a DIFFERENT table for a
//! different kind of standing decision: `core.fleet_approval_gates` records a
//! human answering about ONE event, and `core.integration_grants` records a
//! human answering about a fleet's relationship with a third party, once, for
//! every event after it. They travel together on the lease path and are not
//! the same question.

use afd_core::id::Uuid7;
use afd_fleet_runtime::config::RepositoryBinding;
use sqlx::Row as _;

use crate::error::{Result, query};
use crate::gate::decision::Status;
use crate::gate::detail::{KIND_REPOSITORY_WRITE, REPOSITORY_WRITE_SPEND_CEILING};
use crate::gate::store::Gates;
use crate::policy::grants::Grants;
use crate::sql;

/// Statement name, for the context a query failure carries.
const CONTEXT_GRANTS: &str = "integration grants";

/// Statement name, for the context the write-gate lookup's failure carries.
const CONTEXT_WRITE_GATE: &str = "approved write gate";

impl Gates {
    /// Every integration `fleet_id` holds a standing grant for.
    ///
    /// One batch read per lease, not one per declared credential: a fleet
    /// declaring six would otherwise pay six round trips for a question the
    /// whole set shares.
    ///
    /// Absent, `pending` and `revoked` rows are all absent from the answer.
    /// The three are deliberately not told apart — a caller that could see
    /// `pending` would be invited to treat it as a maybe, and the point of a
    /// standing decision is that only `approved` admits anything.
    ///
    /// Answers the SET rather than a predicate, because the assembly asks once
    /// per declared credential and a predicate would put the round trip back.
    ///
    /// # Errors
    /// Reports a datastore that would not answer. A fleet holding no grants is
    /// [`Grants::none`], not an error.
    pub async fn approved_integrations(&self, fleet_id: &Uuid7) -> Result<Grants> {
        let mut connection = self.database.acquire().await?;
        let rows = sqlx::query(sql::grant::SELECT_APPROVED_SERVICES)
            .bind(fleet_id.as_str())
            .bind(sql::grant::STATUS_APPROVED)
            .fetch_all(&mut *connection)
            .await
            .map_err(query(CONTEXT_GRANTS))?;

        rows.iter()
            .map(|row| row.try_get::<String, _>(0).map_err(query(CONTEXT_GRANTS)))
            .collect::<Result<Vec<_>>>()
            .map(|services| services.into_iter().collect())
    }
}

impl Gates {
    /// The approved repository-write gate this lease may author a branch from.
    ///
    /// `None` means no branch, and every caller reads that as "this write-bound
    /// fleet does not run". The four ways to get it are deliberately
    /// indistinguishable to the caller, because the remedy is the same for all
    /// of them — a human has to approve the reach the fleet currently declares:
    ///
    /// - no gate was raised, or none was approved;
    /// - one was approved after its own deadline had passed;
    /// - one was approved with no recorded reach, or under a different spend
    ///   ceiling than this build raises;
    /// - one was approved for a reach the fleet's config no longer matches.
    ///
    /// The last is the drift check, and it is why the recorded binding comes
    /// back with the identifier rather than being trusted: a fleet that added
    /// a repository since the approval would otherwise write to one nobody was
    /// asked about.
    ///
    /// # Errors
    /// Reports a datastore that would not answer. Every other outcome is
    /// `Ok(None)` — a fleet with no usable approval is not a fault.
    pub async fn approved_write_gate(
        &self,
        fleet_id: &Uuid7,
        event_id: &str,
        binding: &RepositoryBinding,
    ) -> Result<Option<Uuid7>> {
        let mut connection = self.database.acquire().await?;
        let row = sqlx::query(sql::gate::SELECT_APPROVED_WRITE_GATE)
            .bind(fleet_id.as_str())
            .bind(event_id)
            .bind(KIND_REPOSITORY_WRITE)
            .bind(Status::Approved.as_str())
            .bind(REPOSITORY_WRITE_SPEND_CEILING)
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_WRITE_GATE))?;
        let Some(row) = row else {
            return Ok(None);
        };

        let stated: String = row.try_get(1).map_err(query(CONTEXT_WRITE_GATE))?;
        if !binding.matches_recorded(&stated) {
            return Ok(None);
        }
        let gate_id: String = row.try_get(0).map_err(query(CONTEXT_WRITE_GATE))?;
        // A gate identifier this daemon cannot parse is not a gate it can name
        // a branch from — and the branch is what the egress rules lock, so a
        // guess here would be a branch nothing admits.
        Ok(Uuid7::parse(&gate_id).ok())
    }
}
