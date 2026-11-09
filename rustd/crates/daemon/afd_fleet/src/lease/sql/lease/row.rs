//! The lease row's one write: the lease, its audit event and the runner's
//! tally, fenced on the claim that earned it.
//!
//! Split from [`super`] when fencing the insert pushed that file past the
//! length cap; the statement and the struct that binds it stay together,
//! because the `$n` order is written once, beside the text it orders.

use af_core::id::Uuid7;
use af_wire::decision::DecisionAssignment;
use jiff::Timestamp;
use jiff_sqlx::ToSqlx as _;

use crate::{Result, error::query};

/// Open a lease, record the event that opened it and bump the runner's lifetime
/// acquired tally, atomically.
///
/// Writing the lease and its audit trail in one statement means an observer can
/// never see a lease with no corresponding event, or the reverse; the tally
/// rides the same statement so the acquired counter can never drift from the
/// rows it counts.
///
/// # Fenced on the claim that earned it
///
/// The `held` arm writes nothing unless the slot still carries this claim's
/// token (`$10`) and has not lapsed (`$11`), and every other arm reads from it.
/// A pass that outlived its claim lost the slot to a runner that took the same
/// entry over, and an unfenced insert would have two runners executing one
/// event. A lost fence writes no row at all, which the caller reads as no-work.
///
/// `held` is an UPDATE rather than a guarded SELECT so the row lock and the
/// predicate are one step: a claim racing it either commits first and fails
/// the recheck, or waits for this insert and then reclaims the lease it wrote.
///
/// # Every lease meters its own run
///
/// The lease's runtime cursor opens at the claim's instant, fresh or reclaim:
/// a reclaimed run bills from when it started, and the dead holder's last slice
/// was billed to the dead lease when it expired.
///
/// The lease stores no copy of the event body: the reclaim path reads it by
/// joining `fleet.events` on its `event_id` key, so the
/// hottest write in the system stops duplicating the largest value in it.
pub const INSERT_LEASE_WITH_EVENT: &str = include_str!("insert_lease_with_event.sql");

/// Everything [`INSERT_LEASE_WITH_EVENT`] needs, by name.
///
/// Seventeen positional parameters, several of them same-typed text that a
/// transposition would compile straight through, and this workspace disables
/// sqlx's `macros` feature deliberately, so there is no compile-time query
/// checking to catch it. Naming the fields is what replaces
/// it: the `$n` order is written ONCE, here, beside the text it orders.
#[derive(Debug)]
pub struct LeaseRow<'a> {
    /// The lease's durable identifier.
    pub lease_id: &'a Uuid7,
    /// The runner taking the work.
    pub runner_id: &'a Uuid7,
    /// The fleet whose slot was claimed.
    pub fleet_id: &'a Uuid7,
    /// The workspace the fleet belongs to.
    pub workspace_id: &'a Uuid7,
    /// The tenant whose wallet was gated and debited.
    pub tenant_id: &'a Uuid7,
    /// The event being leased, as the admission ledger names it. The statement
    /// resolves it to the `fleet.events` row the lease references.
    pub event_id: &'a Uuid7,
    /// When the event was raised, by the producer's clock; what the lease bills
    /// against.
    pub event_created_at: Timestamp,
    /// The resolved billing posture, as its wire spelling.
    pub posture: &'a str,
    /// The provider resolved at billing, so renew and report key the rate row
    /// by `(provider, model)` without re-resolving.
    pub provider: &'a str,
    /// The model resolved at billing.
    pub model: &'a str,
    /// The claim's fencing token — the value every report is checked against.
    pub fencing_token: i64,
    /// The status the row opens in.
    pub status: &'a str,
    /// The database's own instant, read once `afd_runner`'s host lock was held.
    pub checked_at: Timestamp,
    /// Identifier of the audit row this write also lands.
    pub event_row_id: &'a Uuid7,
    /// Whether this lease is a fresh pull or a reclaim, as its wire spelling.
    /// It reaches the audit row's metadata, where an operator reads it.
    pub kind: &'a str,
    /// The non-secret reviewed selection delivered by this exact lease.
    pub decision_snapshot: Option<&'a DecisionAssignment>,
}

impl<'a> LeaseRow<'a> {
    /// Return the actual stored creation time, or None when the guarded insert lost its claim.
    ///
    /// # Errors
    /// Preserves the statement's failure; no additional timestamp read is issued.
    pub async fn insert(&self, connection: &mut sqlx::PgConnection) -> Result<Option<Timestamp>> {
        let metadata = std::collections::BTreeMap::from([
            (afd_runner::sql::meta::LEASE_ID, self.lease_id.as_str()),
            (afd_runner::sql::meta::FLEET_ID, self.fleet_id.as_str()),
            (
                afd_runner::sql::meta::AGENTSFLEET_EVENT_ID,
                self.event_id.as_str(),
            ),
            (afd_runner::sql::meta::KIND, self.kind),
        ]);
        let created_at = sqlx::query_scalar::<_, jiff_sqlx::Timestamp>(INSERT_LEASE_WITH_EVENT)
            .bind(self.lease_id)
            .bind(self.runner_id)
            .bind(self.fleet_id)
            .bind(self.workspace_id)
            .bind(self.tenant_id)
            .bind(self.event_id)
            .bind(self.posture)
            .bind(self.provider)
            .bind(self.model)
            .bind(self.fencing_token)
            .bind(self.checked_at.to_sqlx())
            .bind(self.status)
            .bind(self.event_row_id)
            .bind(afd_runner::sql::event_type::LEASE_ACQUIRED)
            .bind(sqlx::types::Json(metadata))
            .bind(self.event_created_at.to_sqlx())
            .bind(self.decision_snapshot.map(sqlx::types::Json))
            .fetch_optional(connection)
            .await
            .map_err(query(
                "write the fenced lease and return its saved creation time",
            ))?;
        Ok(created_at.map(|created| created.to_jiff()))
    }
}
