//! The reads a recorded gate is resolved through: two Dragonfly keys and one row.
//!
//! Every key shape this module writes or reads is declared in [`key`], once.
//! The sweeper, the resolver and the webhook handler have to agree on the exact
//! bytes or a pending gate becomes unreachable, and a prefix spelled inline at
//! one call site is precisely how that agreement ends (RULE UFS).

use std::time::Duration;

use afd_api_wire::approval::GateStatus;
use afd_core::id::Uuid7;
use afd_core::spelling::from_spelling;
use afd_crypto::entropy::Entropy;
use afd_db::Db;
use afd_dragonfly::Dragonfly;

use super::sql;
use crate::error::{DETAIL_GATE_REFERENCE_UNWRITABLE, Result, encoded, query};
use crate::gate::decision::{Answer, answer};
use crate::gate::pending::{Evaluation, GateRef, evaluate};

/// Statement name, for the context a query failure carries.
const CONTEXT_STATUS: &str = "approval gate status";

/// An event's durable reference when its cached reference has expired.
const CONTEXT_REFERENCE: &str = "approval gate event reference";

/// The durable fallback answered where the mirror did not.
const EVENT_DB_FALLBACK: &str = "approval_decision_db_fallback_used";

/// How long a reference outlives the timeout it was raised with.
///
/// A re-encountered event has to be able to resolve its gate AFTER the deadline
/// passes — that is what produces the timeout — so the key cannot expire at the
/// deadline. The grace covers the sweeper's cadence plus the gap between the
/// database stamping the deadline and Dragonfly starting the key's clock.
const REF_GRACE: Duration = Duration::from_mins(10);

/// The floor a reference's lifetime is clamped to.
///
/// A gate raised with a zero timeout would otherwise get a key that dies before
/// the poll which resolves it.
const REF_MINIMUM: Duration = Duration::from_hours(2);

/// Every Dragonfly key the approval gate uses.
///
/// Prefixes rather than formatted keys, because two of the three are also read
/// by the sweeper and the webhook handler — a shape declared here and rebuilt
/// there is a shape that can drift by one colon.
pub mod key {
    /// `{fleet}:{event}` → the reference a parked event resolves through.
    const EVENT_REF_PREFIX: &str = "fleet:gate:byevent:";

    /// `{action}` → the mirrored decision.
    const RESPONSE_PREFIX: &str = "fleet:gate:response:";

    /// `{fleet}:{tool}:{action}` → the anomaly window's counter.
    const ANOMALY_PREFIX: &str = "fleet:anomaly:";

    /// The key holding `event_id`'s gate reference.
    #[must_use]
    pub fn event_ref(fleet_id: &str, event_id: &str) -> String {
        format!("{EVENT_REF_PREFIX}{fleet_id}:{event_id}")
    }

    /// The key holding `action_id`'s mirrored decision.
    #[must_use]
    pub fn response(action_id: &str) -> String {
        format!("{RESPONSE_PREFIX}{action_id}")
    }

    /// The key counting one `(fleet, tool, action)` inside its window.
    #[must_use]
    pub fn anomaly(fleet_id: &str, tool: &str, action: &str) -> String {
        format!("{ANOMALY_PREFIX}{fleet_id}:{tool}:{action}")
    }
}

/// The gate reads, over both datastores.
///
/// Both, because a gate genuinely spans them: the question and its answer are
/// durable in Postgres, and the hot-path mirror plus the event reference are in
/// Dragonfly. A store holding one of them would leave the fallback — the whole
/// reason the durable read exists — impossible to express.
#[derive(Debug, Clone)]
pub struct Gates {
    pub(super) database: Db,
    queue: Dragonfly,
    entropy: Entropy,
}

impl Gates {
    /// Gate reads and writes through `database` and `queue`.
    ///
    /// The entropy source is the third, and it arrives for one reason: raising
    /// a gate mints two identifiers — the action a human answers about and the
    /// row that records it — and they are drawn through the workspace's one
    /// entropy surface rather than a second source with its own failure mode.
    /// [`Leases`](crate::lease::Leases) takes it for the same reason.
    #[must_use]
    pub const fn new(database: Db, queue: Dragonfly, entropy: Entropy) -> Self {
        Self {
            database,
            queue,
            entropy,
        }
    }

    /// The queue these gates are mirrored in, for the sibling module that
    /// counts anomalies through it.
    pub(super) const fn queue(&self) -> &Dragonfly {
        &self.queue
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
    /// A missing or corrupt cached reference falls back to the durable event
    /// association. A poll can resume after the cache lifetime has ended; the
    /// recorded question still decides that event, even after a policy edit.
    ///
    /// # Errors
    /// Reports an unreadable queue, database or stored identifier. The caller
    /// must not treat an unreadable reference as an event with no prior gate.
    pub async fn recorded(&self, fleet_id: &Uuid7, event_id: &str) -> Result<Option<GateRef>> {
        let stored = self
            .queue
            .get_string(&key::event_ref(fleet_id.as_str(), event_id))
            .await?;
        if let Some(reference) = stored.and_then(|raw| serde_json::from_str(&raw).ok()) {
            return Ok(Some(reference));
        }
        let mut connection = self.database.acquire().await?;
        let action: Option<String> = sqlx::query_scalar(sql::SELECT_EVENT_GATE)
            .bind(fleet_id.as_str())
            .bind(event_id)
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_REFERENCE))?;
        action
            .map(|id| Uuid7::parse(&id).map(GateRef::new).map_err(Into::into))
            .transpose()
    }

    /// Record that `event_id` is waiting on `reference`, for long enough to
    /// outlive a gate raised with `timeout`.
    ///
    /// # Errors
    /// Reports a queue that would not answer.
    pub async fn record(
        &self,
        fleet_id: &Uuid7,
        event_id: &str,
        reference: &GateRef,
        timeout: Duration,
    ) -> Result<()> {
        let lifetime = reference_lifetime(timeout);
        // An unwritable reference would leave a parked event unable to find
        // its own gate, which is the one outcome worth an error.
        let encoded = encoded(reference, DETAIL_GATE_REFERENCE_UNWRITABLE)?;
        self.queue
            .set_for(
                &key::event_ref(fleet_id.as_str(), event_id),
                &encoded,
                lifetime,
            )
            .await
            .map_err(Into::into)
    }

    /// What this poll makes of `reference`.
    ///
    /// Reads the mirror first and the durable row only when the mirror is
    /// silent, which is one round trip on the path that runs and two on the
    /// path that should not. See the module note on [`super::pending`] for why
    /// the fallback exists at all. The durable read also answers whether the
    /// deadline has passed, on the database clock — a gate still waiting is
    /// exactly the one whose answer the mirror does not hold.
    ///
    /// # Errors
    /// Reports a queue or datastore that would not answer.
    pub async fn evaluate(&self, reference: &GateRef) -> Result<Evaluation> {
        let action = reference.action_id().as_str();
        let mirrored = self.queue.get_string(&key::response(action)).await?;
        if let Some(answer) = mirrored.as_deref().and_then(Answer::parse) {
            return Ok(evaluate(Some(answer), false));
        }

        // A reference whose row cannot be found has no deadline to lapse: the
        // park writes the row before the reference, so only a purge removes
        // it. Waiting is the fail-safe direction, and the key's own lifetime
        // ends the wait.
        let Some(standing) = self.standing(action).await? else {
            return Ok(Evaluation::Pending);
        };
        let durable = standing.status.and_then(answer);
        if let Some(answer) = durable {
            // Only reachable in the window a mirror write missed, which is what
            // makes it worth a line: this is the metric that says the write
            // side has a gap, and it is silent when it does not.
            // Hoisted: see the `tracing` note in the workspace Cargo.toml.
            let outcome = answer.as_str();
            tracing::info!(
                event = EVENT_DB_FALLBACK,
                action_id = action,
                outcome,
                "the decision mirror was absent and the durable row answered"
            );
        }
        Ok(evaluate(durable, standing.lapsed))
    }

    /// Where `action_id`'s newest gate stands, or nothing if it has no row.
    async fn standing(&self, action_id: &str) -> Result<Option<Standing>> {
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

/// How long a reference should live, in the whole seconds Dragonfly's `EX`
/// takes.
///
/// The timeout plus a grace, floored at the minimum. A span and not a
/// deadline: the database stamped the deadline as `now()` plus this same
/// timeout, so the key outlives it by the grace without either side reading a
/// clock to compare. This is the one place the span becomes a count.
///
/// Saturating: a timeout at the edge of the range must cost a clamped lifetime
/// rather than an overflow that aborts the daemon.
fn reference_lifetime(timeout: Duration) -> i64 {
    let lifetime = timeout.saturating_add(REF_GRACE).max(REF_MINIMUM);
    i64::try_from(lifetime.as_secs()).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests;
