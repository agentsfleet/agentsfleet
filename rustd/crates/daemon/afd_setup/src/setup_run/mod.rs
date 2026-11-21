//! A draft's setup turn on the common run path: the draft's half of the
//! assignment, the report and the live answer.
//!
//! The run bookkeeping (thread, fence, run row, renewal, settlement) is the
//! shared statements' in `afd_fleet::run::sql::shared`, which `afd_fleet`
//! runs. What lives here is what reads or writes the draft's own rows, inside
//! the transaction `afd_fleet` holds: which message an assignment takes and what it
//! carries, the message's start and finish, and the one candidate a
//! report may write. The runner makes every model call; this crate makes
//! none.

mod context;
mod failing;
mod lock;
pub(crate) mod model;
mod preview;
mod progress;
mod refusal;
mod report;
mod sql;
mod tool;
mod turn;
mod usage;

pub(crate) use failing::reason_word;
pub use failing::{EVENT_TURN_FAILED, log_turn_failed};
pub use progress::{CONTEXT_EVENT, ContextNotice, DELTA_EVENT, Delta, TextKind};
pub(crate) use sql::DELETE_RUNS;
pub use tool::setup_tool;
pub(crate) use usage::Metered;

use af_core::id::Uuid7;
use af_wire::setup::FailureReason;
use afd_credential::provider::RegisteredModel;
use jiff::Timestamp;
use sqlx::PgConnection;

pub use context::ContextSave;
pub use preview::PreviewChunk;
pub use report::{SetupFinish, SetupReportScope};
pub use turn::{SetupHead, SetupTurn, SetupTurnText};

use self::failing::Failing;
use self::lock::{DraftLock, Hint};
use crate::{
    Drafts, Result,
    draft::read,
    error::raise::query,
    funnel::Owed,
    types::{DraftState, EDITING, MessageState, QUEUED, RUNNING},
};

/// A draft an assignment may try, read without a lock.
#[derive(Debug, Clone)]
pub struct SetupHint {
    /// The draft, which is also its thread's id.
    pub draft_id: Uuid7,
    /// The tenant whose host lock the assignment takes for it.
    pub tenant_id: Uuid7,
    workspace_id: Uuid7,
    user_id: Uuid7,
}

/// [`sql::CANDIDATES`]'s row, by column name.
#[derive(sqlx::FromRow)]
struct HintRow {
    id: Uuid7,
    workspace_id: Uuid7,
    tenant_id: Uuid7,
    user_id: Uuid7,
}

/// [`sql::HEAD`]'s row, by column name.
#[derive(sqlx::FromRow)]
struct HeadRow {
    id: Uuid7,
    sequence: i64,
    status: String,
    tenant_model_id: Option<Uuid7>,
    user_text: String,
    save_requested: bool,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    created_at: Timestamp,
}

/// What assigning one candidate found.
#[derive(Debug)]
pub enum SetupAssignment {
    /// The draft is locked and its head message assigned.
    Assigned(Box<SetupTurn>),
    /// Nothing this draft may run to this runner now.
    Passed,
    /// Another transaction holds the draft or its creator.
    Busy,
}

/// Funnel steps an assignment or report owes once its transaction commits.
#[derive(Debug, Default)]
#[must_use = "owed steps are reported only by `settle`, after the commit"]
pub struct SetupOwed(Owed);

impl SetupOwed {
    /// Reports every owed step. Call only after the transaction committed.
    pub fn settle(self, drafts: &Drafts) {
        self.0.settle(&drafts.analytics);
    }
}

impl Drafts {
    /// Drafts whose head message a draft run may take for `runner`, oldest
    /// head first. Hints only: [`Drafts::assign_setup_turn`] rereads each.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn setup_candidates(&self, runner: &Uuid7) -> Result<Vec<SetupHint>> {
        let rows: Vec<HintRow> = sqlx::query_as(sql::CANDIDATES)
            .bind(EDITING)
            .bind(QUEUED)
            .bind(RUNNING)
            .bind(sql::SCAN_LIMIT)
            .bind(runner)
            .fetch_all(&mut *self.database.acquire().await?)
            .await
            .map_err(query(sql::CONTEXT))?;
        Ok(rows
            .into_iter()
            .map(|row| SetupHint {
                draft_id: row.id,
                tenant_id: row.tenant_id,
                workspace_id: row.workspace_id,
                user_id: row.user_id,
            })
            .collect())
    }

    /// Locks `hint`'s draft and assigns its head message in the assignment's
    /// transaction: the creator still holds the draft, no other turn holds it,
    /// the message's model resolves, and the payer's balance covers a turn.
    ///
    /// A tenant's runner (`tenant_host`) never receives the platform model's
    /// key, so a head on the platform default passes, and blocks its draft for
    /// that runner rather than being jumped. A refused model or balance, a
    /// stored key or selection that is wrong, and a head candidate this build
    /// cannot read each fail the message durably and pass the draft, so one
    /// draft's fault never stalls the walk.
    ///
    /// A running head is reassigned whether or not a run row still names it:
    /// a runner's deletion cascades its runs away, and the head would
    /// otherwise wait forever.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn assign_setup_turn(
        &self,
        connection: &mut PgConnection,
        hint: &SetupHint,
        tenant_host: bool,
        owed: &mut SetupOwed,
    ) -> Result<SetupAssignment> {
        let locking = Hint {
            id: &hint.draft_id,
            workspace_id: &hint.workspace_id,
            tenant_id: &hint.tenant_id,
            user_id: &hint.user_id,
        };
        let draft = match lock::lock_draft(connection, &locking, &mut owed.0).await? {
            DraftLock::Locked(draft) => *draft,
            DraftLock::Passed => return Ok(SetupAssignment::Passed),
            DraftLock::Busy => return Ok(SetupAssignment::Busy),
        };
        if draft.status != DraftState::Editing || lock::running(connection, &draft.id).await? {
            return Ok(SetupAssignment::Passed);
        }
        let Some(head) = head(connection, &draft).await? else {
            return Ok(SetupAssignment::Passed);
        };
        if tenant_host && head.tenant_model_id.is_none() {
            return Ok(SetupAssignment::Passed);
        }
        let reassigned = head.status == RUNNING;
        let failing = Failing {
            message: crate::funnel::Message {
                workspace: &draft.workspace_id,
                creator: &draft.user_id,
                draft: &draft.id,
            },
            sequence: head.sequence,
            running: reassigned,
        };
        let reference = model::reference(head.tenant_model_id.clone());
        let model = match self.setup_model(connection, &draft, &reference).await {
            Ok(model) => model,
            Err(error) => return failing.refuse(connection, error, owed).await,
        };
        let candidate = match head_candidate(connection, &draft).await {
            Ok(candidate) => candidate,
            Err(error) => return failing.refuse(connection, error, owed).await,
        };
        let history = history(connection, &draft.id, head.sequence).await?;
        Ok(SetupAssignment::Assigned(Box::new(SetupTurn {
            draft_id: draft.id.clone(),
            workspace_id: draft.workspace_id.clone(),
            tenant_id: draft.tenant_id.clone(),
            user_id: draft.user_id.clone(),
            message_id: head.id,
            sequence: head.sequence,
            message_created_at: head.created_at,
            user_text: head.user_text,
            save_requested: head.save_requested,
            reassigned,
            model,
            candidate,
            history,
        })))
    }

    /// The message's model, resolved for the payer, and the payer's balance
    /// checked against the run's first slice. A check only: nothing is set
    /// aside.
    async fn setup_model(
        &self,
        connection: &mut PgConnection,
        draft: &read::Locked,
        reference: &str,
    ) -> Result<RegisteredModel> {
        let model = model::selected(connection, &draft.tenant_id, reference).await?;
        model::gate(
            connection,
            &draft.tenant_id,
            model::first_slice(self.clock)?,
        )
        .await?;
        Ok(model)
    }

    /// Marks the assigned message running, in the assignment's transaction, once
    /// its run is written. A reassigned message is running already.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn start_setup_turn(
        &self,
        connection: &mut PgConnection,
        turn: &SetupTurn,
    ) -> Result<()> {
        sqlx::query(sql::RUNNING)
            .bind(&turn.draft_id)
            .bind(turn.sequence)
            .bind(RUNNING)
            .bind(QUEUED)
            .execute(connection)
            .await
            .map_err(query(sql::CONTEXT))?;
        Ok(())
    }

    /// Fails the assigned message durably, in the assignment's transaction: its
    /// runs lapsed too often for another reassign
    /// ([`FailureReason::ProviderUsageUnknown`], a lost turn's reason), or its model's key no longer opens
    /// ([`FailureReason::ModelUnavailable`]).
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn fail_setup_turn(
        &self,
        connection: &mut PgConnection,
        turn: &SetupTurn,
        reason: FailureReason,
        owed: &mut SetupOwed,
    ) -> Result<()> {
        let failing = Failing {
            message: crate::funnel::Message {
                workspace: &turn.workspace_id,
                creator: &turn.user_id,
                draft: &turn.draft_id,
            },
            sequence: turn.sequence,
            running: turn.reassigned,
        };
        failing.fail(connection, reason, owed).await
    }
}

async fn head(connection: &mut PgConnection, draft: &read::Locked) -> Result<Option<HeadRow>> {
    sqlx::query_as(sql::HEAD)
        .bind(&draft.id)
        .bind(QUEUED)
        .bind(RUNNING)
        .fetch_optional(connection)
        .await
        .map_err(query(sql::CONTEXT))
}

/// The draft's head candidate as an assignment carries it, if it has one.
async fn head_candidate(
    connection: &mut PgConnection,
    draft: &read::Locked,
) -> Result<Option<SetupHead>> {
    let Some(id) = &draft.head_candidate_id else {
        return Ok(None);
    };
    let candidate = crate::candidate::read(connection, &draft.id, id).await?;
    Ok(Some(SetupHead::try_from(candidate)?))
}

async fn history(
    connection: &mut PgConnection,
    draft: &Uuid7,
    sequence: i64,
) -> Result<Vec<SetupTurnText>> {
    let rows: Vec<(i64, String, String)> = sqlx::query_as(sql::HISTORY)
        .bind(draft)
        .bind(sequence)
        .bind(MessageState::Completed.as_str())
        // The most earlier turns an assignment carries, as a fleet chat's does.
        .bind(i64::try_from(af_wire::run::HISTORY_TURNS_MAX).unwrap_or(i64::MAX))
        .fetch_all(connection)
        .await
        .map_err(query(sql::CONTEXT))?;
    Ok(rows
        .into_iter()
        .rev()
        .map(|(sequence, message, answer)| SetupTurnText {
            sequence,
            message,
            answer,
        })
        .collect())
}

#[cfg(test)]
mod tests;
