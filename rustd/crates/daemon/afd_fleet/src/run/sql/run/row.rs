//! The run row's one write: the run and its audit event, fenced on the
//! assignment that earned it.
//!
//! Split from [`super`] when fencing the insert pushed that file past the
//! length cap; the statement and the struct that binds it stay together,
//! because the `$n` order is written once, beside the text it orders.

use af_core::id::Uuid7;
use af_wire::decision::DecisionAssignment;
use jiff::Timestamp;
use jiff_sqlx::ToSqlx as _;

use crate::{Result, error::query};

/// Open a run and record the event that opened it, atomically.
///
/// Writing the run and its audit trail in one statement means an observer can
/// never see a run with no corresponding event, or the reverse. The runner's
/// acquired count is a metric the caller records once the issue commits.
///
/// # Fenced on the assignment that earned it
///
/// The `held` arm writes nothing unless the thread still carries this assignment's
/// token (`$10`) and has not lapsed (`$11`), and every other arm reads from it.
/// A pass that outlived its assignment lost the thread to a runner that took the same
/// entry over, and an unfenced insert would have two runners executing one
/// event. A lost fence writes no row at all, which the caller reads as no-work.
///
/// `held` is an UPDATE rather than a guarded SELECT so the row lock and the
/// predicate are one step: an assignment racing it either commits first and fails
/// the recheck, or waits for this insert and then reassigns the run it wrote.
///
/// # Every run meters its own run
///
/// The run's runtime cursor opens at the assignment's instant, fresh or reassignment:
/// a reassigned run bills from when it started, and the dead holder's last slice
/// was billed to the dead run when it expired.
///
/// The run stores no copy of the event body: the reassignment path reads it by
/// joining `thread.turns` on the run's `turn_id`, so the
/// hottest write in the system stops duplicating the largest value in it.
pub const INSERT_RUN_WITH_TURN: &str = include_str!("insert_run_with_turn.sql");

/// Everything [`INSERT_RUN_WITH_TURN`] needs, by name.
///
/// Seventeen positional parameters, several of them same-typed text that a
/// transposition would compile straight through, and this workspace disables
/// sqlx's `macros` feature deliberately, so there is no compile-time query
/// checking to catch it. Naming the fields is what replaces
/// it: the `$n` order is written ONCE, here, beside the text it orders.
#[derive(Debug)]
pub struct RunRow<'a> {
    /// The run's durable identifier.
    pub run_id: &'a Uuid7,
    /// The runner taking the work.
    pub runner_id: &'a Uuid7,
    /// The fleet whose thread was assigned.
    pub fleet_id: &'a Uuid7,
    /// The workspace the fleet belongs to.
    pub workspace_id: &'a Uuid7,
    /// The tenant whose wallet was gated and debited.
    pub tenant_id: &'a Uuid7,
    /// The event being held: the turn's id. The statement resolves it to the
    /// `thread.turns` row the run references.
    pub turn_id: &'a Uuid7,
    /// When the event was raised, by the producer's clock; what the run bills
    /// against.
    pub turn_created_at: Timestamp,
    /// The resolved billing posture, as its wire spelling.
    pub posture: &'a str,
    /// The provider resolved at billing, so renew and report key the rate row
    /// by `(provider, model)` without re-resolving.
    pub provider: &'a str,
    /// The model resolved at billing.
    pub model: &'a str,
    /// The assignment's fencing token — the value every report is checked against.
    pub fencing_token: i64,
    /// The status the row opens in.
    pub status: &'a str,
    /// Identifier of the audit row this write also lands.
    pub event_row_id: &'a Uuid7,
    /// Whether this run is a fresh pull or a reassignment, as its wire spelling.
    /// It reaches the audit row's metadata, where an operator reads it.
    pub kind: &'a str,
    /// The non-secret reviewed selection delivered by this exact run.
    pub decision_snapshot: Option<&'a DecisionAssignment>,
    /// The placement choice's revision the host lock read; the insert
    /// advances it, and writes nothing that commits if it moved.
    pub run_version: i64,
    /// The kind the advanced choice records.
    pub run_kind: &'a str,
    /// When the assignment lapses, as the assignment answered it: a pass that outlived
    /// it writes no run.
    pub held_until: Timestamp,
    /// The head revision the assignment assembled the setup from; a run whose
    /// fleet's head moved since writes nothing. `None` checks no revision.
    pub setup_revision: Option<&'a Uuid7>,
}

impl RunRow<'_> {
    /// Return the actual stored creation time, or None when the guarded insert lost its assignment.
    ///
    /// # Errors
    /// Preserves the statement's failure; no additional timestamp read is issued.
    pub async fn insert(&self, connection: &mut sqlx::PgConnection) -> Result<Option<Timestamp>> {
        let metadata = std::collections::BTreeMap::from([
            (afd_runner::sql::meta::RUN_ID, self.run_id.as_str()),
            (afd_runner::sql::meta::FLEET_ID, self.fleet_id.as_str()),
            (afd_runner::sql::meta::TURN_ID, self.turn_id.as_str()),
            (afd_runner::sql::meta::KIND, self.kind),
        ]);
        let created_at = sqlx::query_scalar::<_, jiff_sqlx::Timestamp>(INSERT_RUN_WITH_TURN)
            .bind(self.run_id)
            .bind(self.runner_id)
            .bind(self.fleet_id)
            .bind(self.workspace_id)
            .bind(self.tenant_id)
            .bind(self.turn_id)
            .bind(self.posture)
            .bind(self.provider)
            .bind(self.model)
            .bind(self.fencing_token)
            .bind(self.status)
            .bind(self.event_row_id)
            .bind(afd_runner::sql::event_type::RUN_ACQUIRED)
            .bind(sqlx::types::Json(metadata))
            .bind(self.turn_created_at.to_sqlx())
            .bind(self.decision_snapshot.map(sqlx::types::Json))
            .bind(self.run_version)
            .bind(self.run_kind)
            .bind(self.held_until.to_sqlx())
            .bind(self.setup_revision)
            .bind(af_wire::task::CAPABILITY)
            .bind(crate::task::DECISION_POLICY_PATH)
            .fetch_optional(connection)
            .await
            .map_err(query(
                "write the fenced run and return its saved creation time",
            ))?;
        Ok(created_at.map(jiff_sqlx::Timestamp::to_jiff))
    }
}
