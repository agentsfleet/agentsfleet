//! A runner effect lands once per call: a message, a schedule write, a
//! run-now.
//!
//! ```text
//!   request(item_id, call_number) ─► lock (run, call_number) ─► item stored? ──yes─► its answer
//!                                                                    │ no
//!                                                                    ▼
//!                                              effect ─► item written with the answer ─► commit
//! ```
//!
//! The request names its call's item: the run and the call's number, the key
//! `thread.items` is unique on. A transaction-scoped advisory lock on that key
//! holds while the effect runs, so a retry sent while the first is in flight
//! waits for it, then reads the answer the first stored and does nothing
//! again. An effect that failed writes no item, so its retry runs it. The lock
//! touches no row: the run's renewals and settlement never wait on a post.
//!
//! The item outlives the run that wrote it until the turn settles, so a run
//! that takes a dead run's turn over reads what it finished
//! ([`Runs::finished_calls`]).

use af_core::id::Uuid7;
use af_wire::run::EarlierCall;
use af_wire::tool_detail::{ItemKind, ItemStatus};
use sqlx::{Postgres, Transaction};

use crate::error::{Result, query};
use crate::run::history::{CallRow, call};
use crate::run::pull::RunService;
use crate::run::sql::effect as statement;
use crate::run::standing::Standing;
use crate::run::store::Runs;

/// Statement name, for the context a query failure carries.
const CONTEXT_CLAIM: &str = "claim an effect's item";

/// Statement name, for the context a query failure carries.
const CONTEXT_RECORD: &str = "record an effect's item";

/// Statement name, for the context a query failure carries.
const CONTEXT_FINISHED: &str = "read a turn's finished calls";

/// A retried call found its item and answered what the first stored.
const EVENT_DEDUPLICATED: &str = "effect_deduplicated";

/// One effect a run asks for, named by its call.
#[derive(Debug, Clone, Copy)]
pub struct Effect<'a> {
    /// The item the runner minted for the call.
    pub item_id: &'a Uuid7,
    /// The call's number within its run.
    pub call_number: u64,
    /// The fence the request presented, which the item keeps.
    pub fencing_token: u64,
    /// What the effect is, as the item names its tool.
    pub tool: &'static str,
}

/// What claiming a call's item found.
#[derive(Debug)]
pub enum Claim {
    /// No item yet: run the effect, then [`Claimed::record`] its answer.
    Fresh(Claimed),
    /// The call already landed; this is the answer it stored.
    Done(String),
}

/// A call no item answers yet, held until its answer is recorded or it is
/// dropped, which lets a retry run the effect.
#[derive(Debug)]
pub struct Claimed {
    transaction: Transaction<'static, Postgres>,
    standing: Standing,
    item_id: Uuid7,
    sequence: i64,
    fence: i64,
    tool: &'static str,
}

impl Claimed {
    /// Writes the call's item with what the effect did, `arguments` (an
    /// object, its secret values masked), and what it answered, then lets a
    /// waiting retry read it.
    ///
    /// # Errors
    /// Reports arguments that do not serialize and a datastore that would not
    /// answer; the item is then not written, and a retry runs the effect again.
    pub async fn record<A: serde::Serialize + Sync>(
        mut self,
        arguments: &A,
        output: &str,
    ) -> Result<()> {
        let arguments =
            serde_json::to_string(arguments).map_err(af_core::json::Unreadable::from)?;
        let lines = i64::try_from(output.lines().count()).unwrap_or(i64::MAX);
        sqlx::query(statement::INSERT_ITEM)
            .bind(&self.item_id)
            .bind(&self.standing.workspace_id)
            .bind(&self.standing.fleet_id)
            .bind(&self.standing.run_id)
            .bind(&self.standing.turn_id)
            .bind(self.fence)
            .bind(self.sequence)
            .bind(ItemKind::ToolCall.as_str())
            .bind(self.tool)
            .bind(ItemStatus::Succeeded.as_str())
            .bind(arguments)
            .bind(output)
            .bind(lines)
            .execute(&mut *self.transaction)
            .await
            .map_err(query(CONTEXT_RECORD))?;
        self.transaction
            .commit()
            .await
            .map_err(query(CONTEXT_RECORD))
    }
}

impl RunService {
    /// Claims the item `effect` names for the run `standing` proved: the
    /// answer it stored, or the hold under which the effect runs once.
    ///
    /// The caller proves the run's standing first, so a superseded holder is
    /// refused before any item is read.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn claim_effect(&self, standing: &Standing, effect: Effect<'_>) -> Result<Claim> {
        let sequence = i64::try_from(effect.call_number).unwrap_or(i64::MAX);
        let connection = self.runs.pool().acquire().await?;
        let mut transaction = Transaction::begin(connection, None)
            .await
            .map_err(query(CONTEXT_CLAIM))?;
        sqlx::query(statement::LOCK_ITEM)
            .bind(&standing.run_id)
            .bind(sequence)
            .execute(&mut *transaction)
            .await
            .map_err(query(CONTEXT_CLAIM))?;
        let stored: Option<String> = sqlx::query_scalar(statement::SELECT_ITEM_OUTPUT)
            .bind(&standing.run_id)
            .bind(sequence)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(query(CONTEXT_CLAIM))?;
        if let Some(output) = stored {
            let (run_id, item_id) = (standing.run_id.as_str(), effect.item_id.as_str());
            let (effect_kind, event) = (effect.tool, EVENT_DEDUPLICATED);
            tracing::info!(run_id, item_id, effect_kind, event);
            return Ok(Claim::Done(output));
        }
        Ok(Claim::Fresh(Claimed {
            transaction,
            standing: standing.clone(),
            item_id: effect.item_id.clone(),
            sequence,
            fence: i64::try_from(effect.fencing_token).unwrap_or(i64::MAX),
            tool: effect.tool,
        }))
    }
}

impl Runs {
    /// Every call another run of `turn` finished, oldest first: what `run`,
    /// taking the turn over, reconciles before it acts again.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn finished_calls(
        &self,
        fleet: &Uuid7,
        turn: &Uuid7,
        run: &Uuid7,
    ) -> Result<Vec<EarlierCall<'static>>> {
        let mut connection = self.pool().acquire().await?;
        let rows: Vec<CallRow> = sqlx::query_as(statement::SELECT_FINISHED_CALLS)
            .bind(fleet)
            .bind(turn)
            .bind(run)
            .fetch_all(&mut *connection)
            .await
            .map_err(query(CONTEXT_FINISHED))?;
        Ok(rows.into_iter().filter_map(call).collect())
    }
}
