//! One grant and at most one decision card, on the caller's transaction.

use super::{
    CONTEXT_REQUEST, GRANT_DECISION_WINDOW, KIND_INTEGRATION_GRANT, REASON_ASKED_BY_THE_FLEET,
    Requested, Wanted,
};
use crate::{Result, error, grant::IntegrationGrants, grant_sql};
use af_core::id::Uuid7;
use afd_api_wire::grant::status;
use jiff::Timestamp;
use sqlx::PgConnection;

/// What the request statement answers: the grant, whether this request asked
/// a person, the card it raised, and the card already open.
#[derive(Debug, sqlx::FromRow)]
pub(super) struct RequestRow {
    pub(super) grant_id: Uuid7,
    pub(super) asked: bool,
    pub(super) raised: Option<Uuid7>,
    pub(super) open: Option<Uuid7>,
}

impl RequestRow {
    /// The outcome a caller acts on.
    ///
    /// A request that asked a person and found neither a card it raised nor
    /// one already open lost a race to a concurrent request whose card this
    /// statement's snapshot could not see: that card stands, so it waits.
    pub(super) fn outcome(self) -> Requested {
        let grant = self.grant_id;
        match (self.raised, self.open) {
            (Some(gate), _) => Requested::Raised { grant, gate },
            (None, Some(gate)) => Requested::Pending {
                grant,
                gate: Some(gate),
            },
            (None, None) if self.asked => Requested::Pending { grant, gate: None },
            (None, None) => Requested::Covered { grant },
        }
    }
}

impl IntegrationGrants {
    pub(super) async fn request_on(
        &self,
        connection: &mut PgConnection,
        workspace: &Uuid7,
        fleet: &Uuid7,
        wanted: &Wanted<'_>,
        now: Timestamp,
    ) -> Result<Requested> {
        // Hoisted, not drawn inside the chain: three identifiers are minted
        // here and each is fallible, so naming them says which row is which and
        // keeps the failure ahead of the statement rather than inside it.
        let grant_id = self.mint(now)?;
        let gate_id = self.mint(now)?;
        let action_id = self.mint(now)?;
        sqlx::query(grant_sql::ENSURE_GRANT)
            .bind(grant_id)
            .bind(fleet)
            .bind(wanted.name)
            .bind(wanted.kind.as_str())
            .bind(status::PENDING)
            .bind(REASON_ASKED_BY_THE_FLEET)
            .execute(&mut *connection)
            .await
            .map_err(error::query(CONTEXT_REQUEST))?;
        let (proposal, radius) = wanted.kind.card(wanted.name);
        let asked = sqlx::query_as::<_, RequestRow>(grant_sql::REQUEST_GRANT)
            .bind(fleet)
            .bind(wanted.name)
            .bind(wanted.kind.as_str())
            .bind(wanted.scope)
            .bind(status::PENDING)
            .bind(status::REVOKED)
            .bind(gate_id)
            .bind(workspace)
            .bind(action_id)
            .bind(wanted.turn)
            .bind(KIND_INTEGRATION_GRANT)
            .bind(proposal)
            .bind(wanted.evidence())
            .bind(radius)
            .bind(GRANT_DECISION_WINDOW)
            .bind(afd_api_wire::approval::status::PENDING)
            .fetch_one(&mut *connection)
            .await
            .map_err(error::query(CONTEXT_REQUEST))?;
        Ok(asked.outcome())
    }
}
