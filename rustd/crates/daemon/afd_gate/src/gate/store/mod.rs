//! The reads a recorded gate is resolved through: the durable `approval_gates`
//! row, and nothing else since M244 moved the event reference off Dragonfly.

use std::sync::Arc;

use af_core::id::Uuid7;
use af_core::spelling::from_spelling;
use af_crypto::entropy::Entropy;
use afd_api_wire::approval::GateStatus;
use afd_bus::Handle;
use afd_db::Db;

use super::anomaly::{ANOMALY_KEYS_MAX, Brake};
use super::sql;
use crate::error::{Result, query};
use crate::gate::decision::answer;
use crate::gate::pending::{Evaluation, GateRef, evaluate};

/// Statement name, for the context a query failure carries.
const CONTEXT_STATUS: &str = "approval gate status";

/// The statement that finds an event's gate.
const CONTEXT_REFERENCE: &str = "approval gate event reference";

/// The gate reads and writes, and this replica's anomaly brake.
///
/// The question, its answer and the event it holds are durable in Postgres; a
/// parked gate announces on the bus. The brake counts in memory and is shared
/// by every clone, so one replica keeps one set of windows.
#[derive(Debug, Clone)]
pub struct Gates {
    pub(super) database: Db,
    bus: Handle,
    entropy: Entropy,
    brake: Arc<Brake>,
}

impl Gates {
    /// Gate reads and writes through `database`, announcing on `bus`.
    ///
    /// The entropy source is the third, and it arrives for one reason: raising
    /// a gate mints two identifiers — the action a human answers about and the
    /// row that records it — and they are drawn through the workspace's one
    /// entropy surface rather than a second source with its own failure mode.
    /// [`Leases`](crate::lease::Leases) takes it for the same reason.
    #[must_use]
    pub fn new(database: Db, bus: Handle, entropy: Entropy) -> Self {
        Self {
            database,
            bus,
            entropy,
            brake: Arc::new(Brake::new(ANOMALY_KEYS_MAX)),
        }
    }

    /// This replica's anomaly windows.
    pub(super) fn brake(&self) -> &Brake {
        &self.brake
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

    /// The gate `event_id` is already waiting on, if any.
    ///
    /// Read from `fleet.approval_gates` on `idx_approval_gates_fleet_id_event_id`:
    /// the recorded question still decides that event, even after a policy edit.
    ///
    /// # Errors
    /// Reports an unreadable database or stored identifier. The caller must not
    /// treat an unreadable reference as an event with no prior gate.
    pub async fn recorded(&self, fleet_id: &Uuid7, event_id: &Uuid7) -> Result<Option<GateRef>> {
        let mut connection = self.database.acquire().await?;
        let action: Option<Uuid7> = sqlx::query_scalar(sql::SELECT_EVENT_GATE)
            .bind(fleet_id)
            .bind(event_id)
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_REFERENCE))?;
        Ok(action.map(GateRef::new))
    }

    /// What this poll makes of `reference`.
    ///
    /// One read of the durable row, which also answers whether the deadline
    /// has passed, on the database clock.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn evaluate(&self, reference: &GateRef) -> Result<Evaluation> {
        // A reference whose row cannot be found has no deadline to lapse: only
        // a purge removes the row. Waiting is the fail-safe direction.
        let Some(standing) = self.standing(reference.action_id()).await? else {
            return Ok(Evaluation::Pending);
        };
        Ok(evaluate(standing.status.and_then(answer), standing.lapsed))
    }

    /// Where `action_id`'s newest gate stands, or nothing if it has no row.
    async fn standing(&self, action_id: &Uuid7) -> Result<Option<Standing>> {
        let mut connection = self.database.acquire().await?;
        let row = sqlx::query_as::<_, StoredStanding>(sql::SELECT_GATE_STATUS)
            .bind(action_id)
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_STATUS))?;

        Ok(row.map(|stored| Standing {
            // A spelling this daemon does not know is treated as no status at
            // all, which leaves the gate PENDING. Guessing in either direction
            // would either release an event on an unrecognised word or kill one.
            status: from_spelling::<GateStatus>(&stored.status),
            lapsed: stored.lapsed,
        }))
    }
}

/// The durable gate row as [`sql::SELECT_GATE_STATUS`] returns it.
#[derive(Debug, sqlx::FromRow)]
struct StoredStanding {
    status: String,
    lapsed: bool,
}

/// Where a gate stands: its status, when this build knows the word, and
/// whether its deadline has passed on the database clock.
#[derive(Debug, Clone, Copy)]
struct Standing {
    status: Option<GateStatus>,
    lapsed: bool,
}
