//! A run that ended asking the person: its question becomes a `question`
//! gate on the turn, in the report's own transaction, so a turn never settles
//! `waiting` with nothing for the person to answer.

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

impl Runs {
    /// Raise `question` as a pending gate on `run`'s turn, due by the
    /// default approval deadline. The deadline is shown, never enforced: the
    /// gate stays answerable until it is answered.
    pub(super) async fn raise_question(
        &self,
        connection: &mut PgConnection,
        run: &Reported,
        question: &str,
        now: Timestamp,
    ) -> Result<()> {
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
            .bind(af_fleet_runtime::config::DEFAULT_TIMEOUT)
            .bind(GateStatus::Pending.as_str())
            .bind(&run.turn_id)
            .fetch_one(connection)
            .await
            .map_err(query(CONTEXT_QUESTION))?;
        Ok(())
    }
}
