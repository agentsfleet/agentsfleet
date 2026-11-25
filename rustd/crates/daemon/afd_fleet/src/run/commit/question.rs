//! A run that ended asking the person: its question becomes a `question`
//! gate on the turn, in the report's own transaction, so a turn never settles
//! `waiting` with nothing for the person to answer.
//!
//! A run that ended waiting on an access request already has its card: the
//! request raised it carrying the turn, and its answer resumes the turn as a
//! question's would. That run raises no second gate.

use std::time::Duration;

use af_core::id::Uuid7;
use afd_api_wire::approval::{GATE_TYPE_QUESTION, GateStatus};
use jiff::Timestamp;
use sqlx::PgConnection;

use crate::error::{Result, query};
use crate::run::settle::Reported;
use crate::run::store::Runs;

/// What a query failure names this write as.
const CONTEXT_QUESTION: &str = "report question gate";

/// The evidence a question carries: none beyond the question itself.
const NO_EVIDENCE: &str = "{}";

/// A question reaches nothing beyond the person asked.
const NO_BLAST_RADIUS: &str = "";

/// The deadline a question's card shows: `fleet.approval_gates.expires_at`
/// is `NOT NULL`, and nothing enforces it, so the gate stays answerable until
/// it is answered. The hour the gate policy defaulted to before it was removed.
const QUESTION_DEADLINE: Duration = Duration::from_hours(1);

/// Whether a turn holds an unanswered grant card.
///
/// `$1` fleet, `$2` turn, `$3` the grant card's gate type.
const OPEN_GRANT_CARD: &str = include_str!("open_grant_card.sql");

impl Runs {
    /// Raise `question` as a pending gate on `run`'s turn, unless the turn
    /// already waits on an access card. Answers whether a gate was raised.
    pub(super) async fn raise_question(
        &self,
        connection: &mut PgConnection,
        run: &Reported,
        question: &str,
        now: Timestamp,
    ) -> Result<bool> {
        let asked_for_access: bool = sqlx::query_scalar(OPEN_GRANT_CARD)
            .bind(&run.fleet_id)
            .bind(&run.turn_id)
            .bind(afd_approval::KIND_INTEGRATION_GRANT)
            .fetch_one(&mut *connection)
            .await
            .map_err(query(CONTEXT_QUESTION))?;
        if asked_for_access {
            return Ok(false);
        }
        let gate_id: Uuid7 = self.entropy().uuid7(now)?;
        let action_id: Uuid7 = self.entropy().uuid7(now)?;
        sqlx::query_scalar::<_, i64>(afd_gate::gate::sql::INSERT_GATE)
            .bind(&gate_id)
            .bind(&run.fleet_id)
            .bind(&run.workspace_id)
            .bind(&action_id)
            .bind(GATE_TYPE_QUESTION)
            .bind(GATE_TYPE_QUESTION)
            .bind(GATE_TYPE_QUESTION)
            .bind(question)
            .bind(NO_EVIDENCE)
            .bind(NO_BLAST_RADIUS)
            .bind(QUESTION_DEADLINE)
            .bind(GateStatus::Pending.as_str())
            .bind(&run.turn_id)
            .fetch_one(connection)
            .await
            .map_err(query(CONTEXT_QUESTION))?;
        Ok(true)
    }
}
