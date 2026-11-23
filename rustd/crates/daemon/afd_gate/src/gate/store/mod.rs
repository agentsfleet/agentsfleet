//! The reads a recorded gate is resolved through: the durable `approval_gates`
//! row, and nothing else.

use af_core::id::Uuid7;
use af_core::spelling::from_spelling;
use af_crypto::entropy::Entropy;
use afd_api_wire::approval::GateStatus;
use afd_bus::Handle;
use afd_db::Db;

use super::sql;
use crate::error::{Result, query};
use crate::gate::decision::answer;
use crate::gate::pending::{Evaluation, GateRef, evaluate};

/// Statement name, for the context a query failure carries.
const CONTEXT_STATUS: &str = "approval gate status";

/// The statement that finds an event's gate.
const CONTEXT_REFERENCE: &str = "approval gate event reference";

/// The gate reads and writes.
///
/// The question, its answer and the event it holds are durable in Postgres; a
/// parked gate announces on the bus.
#[derive(Debug, Clone)]
pub struct Gates {
    pub(super) database: Db,
    bus: Handle,
    entropy: Entropy,
}

impl Gates {
    /// Gate reads and writes through `database`, announcing on `bus`.
    ///
    /// The entropy source is the third, and it arrives for one reason: raising
    /// a gate mints two identifiers — the action a human answers about and the
    /// row that records it — and they are drawn through the workspace's one
    /// entropy surface rather than a second source with its own failure mode.
    /// [`Runs`](crate::run::Runs) takes it for the same reason.
    #[must_use]
    pub fn new(database: Db, bus: Handle, entropy: Entropy) -> Self {
        Self {
            database,
            bus,
            entropy,
        }
    }

    /// The bus a parked gate announces on.
    pub(super) const fn bus(&self) -> &Handle {
        &self.bus
    }

    /// The datastore the durable half of a gate lives in.
    pub(super) const fn database(&self) -> &Db {
        &self.database
    }

    /// The entropy source a raised gate draws its identifiers from.
    pub(super) const fn entropy(&self) -> &Entropy {
        &self.entropy
    }

    /// The gate `turn_id` is already waiting on, if any.
    ///
    /// Read from `fleet.approval_gates` on `idx_approval_gates_fleet_id_turn_id`:
    /// the recorded question still decides that event, even after a policy edit.
    ///
    /// # Errors
    /// Reports an unreadable database or stored identifier. The caller must not
    /// treat an unreadable reference as an event with no prior gate.
    pub async fn recorded(&self, fleet_id: &Uuid7, turn_id: &Uuid7) -> Result<Option<GateRef>> {
        let mut connection = self.database.acquire().await?;
        let action: Option<Uuid7> = sqlx::query_scalar(sql::SELECT_EVENT_GATE)
            .bind(fleet_id)
            .bind(turn_id)
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_REFERENCE))?;
        Ok(action.map(GateRef::new))
    }

    /// What this poll makes of `reference`.
    ///
    /// One read of the durable row.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn evaluate(&self, reference: &GateRef) -> Result<Evaluation> {
        // A reference whose row cannot be found is unanswered: only a purge
        // removes the row. Waiting is the fail-safe direction.
        let status = self.standing(reference.action_id()).await?;
        Ok(evaluate(status.and_then(answer)))
    }

    /// Where `action_id`'s newest gate stands, or nothing if it has no row or
    /// its status is a word this build does not know.
    async fn standing(&self, action_id: &Uuid7) -> Result<Option<GateStatus>> {
        let mut connection = self.database.acquire().await?;
        let status: Option<String> = sqlx::query_scalar(sql::SELECT_GATE_STATUS)
            .bind(action_id)
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_STATUS))?;

        // A spelling this daemon does not know is treated as no status at
        // all, which leaves the gate PENDING. Guessing in either direction
        // would either release an event on an unrecognised word or kill one.
        Ok(status.as_deref().and_then(from_spelling::<GateStatus>))
    }
}
