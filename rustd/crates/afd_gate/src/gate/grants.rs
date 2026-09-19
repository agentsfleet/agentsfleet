//! Which integrations a fleet may mint against.
//!
//! Separate from [`super::store`] because it reads a DIFFERENT table for a
//! different kind of standing decision: `core.fleet_approval_gates` records a
//! human answering about ONE event, and `core.integration_grants` records a
//! human answering about a fleet's relationship with a third party, once, for
//! every event after it. They travel together on the lease path and are not
//! the same question.

use afd_core::id::Uuid7;
use sqlx::Row as _;

use super::grant_sql;
use crate::error::{Result, query};
use crate::gate::store::Gates;
use crate::policy::grants::Grants;

/// Statement name, for the context a query failure carries.
const CONTEXT_GRANTS: &str = "integration grants";

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
        let rows = sqlx::query(grant_sql::SELECT_APPROVED_SERVICES)
            .bind(fleet_id.as_str())
            .bind(grant_sql::STATUS_APPROVED)
            .fetch_all(&mut *connection)
            .await
            .map_err(query(CONTEXT_GRANTS))?;

        rows.iter()
            .map(|row| row.try_get::<String, _>(0).map_err(query(CONTEXT_GRANTS)))
            .collect::<Result<Vec<_>>>()
            .map(|services| services.into_iter().collect())
    }
}
