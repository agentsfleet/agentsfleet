//! A draft's setup turn on the common run path: the draft's half of the
//! assignment, the report and the live answer.
//!
//! The run bookkeeping (thread, fence, run row, renewal, settlement) is the
//! shared statements' in `afd_fleet::run::sql::shared`, which `afd_fleet`
//! runs. What lives here is what reads or writes the draft's own rows, inside
//! the transaction `afd_fleet` holds: which message an assignment takes and what it
//! carries, the message's start and finish, and the one candidate a
//! report may write. The runner makes every model call; this crate makes none
//! on this path.
//!
//! The authoring attempt path (`crate::run`) runs beside this one until the
//! runner's setup agent replaces it. Each path refuses a draft the other holds:
//! a live attempt here, a live draft thread there.

mod preview;
mod report;
mod sql;

pub(crate) use sql::DELETE_RUNS;

use af_core::{error_code, id::Uuid7};
use af_wire::authoring::FailureReason;
use afd_credential::provider::RegisteredModel;
use jiff::Timestamp;
use sqlx::PgConnection;

pub use preview::PreviewChunk;
pub use report::{SetupFinish, SetupReportScope};

use crate::{
    Drafts, Result,
    draft::read,
    error::raise::query,
    funnel::Owed,
    model::{self, budget},
    run::{
        FINISHED,
        assign::selection::{self, DraftLock, Hint},
        pending,
        report::verdict,
    },
    types::{CANCELLED, Candidate, DraftState, EDITING, MessageState, QUEUED, RUNNING},
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
    held: bool,
}

/// One earlier turn of the draft's conversation. Its `Debug` hides both
/// texts: they are the creator's private conversation.
#[derive(Clone)]
pub struct SetupTurnText {
    /// What the person asked.
    pub message: String,
    /// What setup answered.
    pub answer: String,
}

/// The turn an assignment took, with everything the assignment carries. Its `Debug`
/// names the draft and message only.
pub struct SetupTurn {
    /// The draft, which is also its thread's id.
    pub draft_id: Uuid7,
    /// The draft's workspace.
    pub workspace_id: Uuid7,
    /// The tenant who pays.
    pub tenant_id: Uuid7,
    /// The draft's creator, who sent the message.
    pub user_id: Uuid7,
    /// The message the run runs: its `turn_id`.
    pub message_id: Uuid7,
    /// The message's position in the draft.
    pub sequence: i64,
    /// When the message was accepted.
    pub message_created_at: Timestamp,
    /// The person's text.
    pub user_text: String,
    /// The person asked to save with this message.
    pub save_requested: bool,
    /// A lapsed draft run ran this message before; this assignment reassigns it.
    pub reassigned: bool,
    /// The model the message pinned, resolved and admitted.
    pub model: RegisteredModel,
    /// The draft's head candidate.
    pub candidate: Option<SetupHead>,
    /// Earlier answered turns, oldest first.
    pub history: Vec<SetupTurnText>,
}

impl std::fmt::Debug for SetupTurnText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SetupTurnText").finish_non_exhaustive()
    }
}

impl std::fmt::Debug for SetupTurn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SetupTurn")
            .field("draft_id", &self.draft_id)
            .field("message_id", &self.message_id)
            .field("sequence", &self.sequence)
            .field("reassigned", &self.reassigned)
            .finish_non_exhaustive()
    }
}

/// The draft's head candidate as an assignment carries it.
#[derive(Debug, Clone)]
pub struct SetupHead {
    /// The candidate a report's base names.
    pub id: Uuid7,
    /// Its revision, which a report's base names too.
    pub revision: u64,
    /// Its Definition, as JSON.
    pub definition: serde_json::Value,
}

impl TryFrom<Candidate> for SetupHead {
    type Error = crate::Error;

    fn try_from(candidate: Candidate) -> Result<Self> {
        Ok(Self {
            revision: candidate
                .revision
                .parse()
                .map_err(|_unreadable| crate::error::raise::absent("revision"))?,
            definition: serde_json::to_value(&candidate.definition)
                .map_err(crate::error::raise::stored)?,
            id: candidate.id,
        })
    }
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

/// The `propose_setup` tool a setup run offers: its description and
/// parameters, for a draft with a candidate to edit (`editing`) or without.
#[must_use]
pub fn setup_tool(editing: bool) -> serde_json::Value {
    model::setup_tool(editing)
}

impl Drafts {
    /// The longest a setup run may run: the turn wall the attempt path caps
    /// a turn at, so one runaway costs minutes.
    #[must_use]
    pub fn setup_max_runtime(&self) -> std::time::Duration {
        self.turn_policy.wall()
    }

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
            .bind(&FINISHED[..])
            .bind(sql::SCAN_LIMIT)
            .bind(runner)
            .bind(CANCELLED)
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
    /// that runner rather than being jumped. A refused model or balance fails
    /// the message durably, as the attempt path does.
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
            id: hint.draft_id.clone(),
            workspace_id: hint.workspace_id.clone(),
            tenant_id: hint.tenant_id.clone(),
            user_id: hint.user_id.clone(),
        };
        let draft = match selection::lock_draft(connection, &locking, &mut owed.0).await? {
            DraftLock::Locked(draft) => *draft,
            DraftLock::Passed => return Ok(SetupAssignment::Passed),
            DraftLock::Busy => return Ok(SetupAssignment::Busy),
        };
        if draft.status != DraftState::Editing || pending::running(connection, &draft).await? {
            return Ok(SetupAssignment::Passed);
        }
        let Some(head) = head(connection, &draft).await? else {
            return Ok(SetupAssignment::Passed);
        };
        let reassigned = head.status == RUNNING;
        // A running head no draft run ran belongs to the attempt path.
        if (reassigned && !head.held) || (tenant_host && head.tenant_model_id.is_none()) {
            return Ok(SetupAssignment::Passed);
        }
        let reference = model::reference(head.tenant_model_id.clone());
        let model = match self.setup_model(connection, &draft, &reference).await {
            Ok(model) => model,
            Err(error) => {
                let reason = match error.code() {
                    error_code::AUTHORING_MODEL => FailureReason::ModelUnavailable,
                    error_code::AUTHORING_BALANCE => FailureReason::InsufficientBalance,
                    _ => return Err(error),
                };
                let failed = Failing {
                    draft: &draft,
                    sequence: head.sequence,
                    running: reassigned,
                };
                failed.fail(connection, reason, owed).await?;
                return Ok(SetupAssignment::Passed);
            }
        };
        let candidate = match &draft.head_candidate_id {
            Some(id) => Some(SetupHead::try_from(
                crate::candidate::read(connection, &draft.id, id).await?,
            )?),
            None => None,
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
    /// checked against one turn's bound. A check only: nothing is set aside.
    async fn setup_model(
        &self,
        connection: &mut PgConnection,
        draft: &read::Locked,
        reference: &str,
    ) -> Result<RegisteredModel> {
        let model = model::selected(connection, &draft.tenant_id, reference).await?;
        let bound = budget::bound(&model, self.turn_policy)?;
        budget::gate(connection, &draft.tenant_id, bound).await?;
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
    /// ([`FailureReason::ProviderUsageUnknown`], the attempt path's reason for
    /// a lost turn), or its model's key no longer opens
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
        let failed = MessageState::Failed.as_str();
        sqlx::query(crate::sql::message::FAIL)
            .bind(&turn.draft_id)
            .bind(turn.sequence)
            .bind(failed)
            .bind(verdict::reason(reason))
            .bind(if turn.reassigned { RUNNING } else { QUEUED })
            .execute(&mut *connection)
            .await
            .map_err(query(sql::CONTEXT))?;
        let message = crate::funnel::Message {
            workspace: &turn.workspace_id,
            creator: &turn.user_id,
            draft: &turn.draft_id,
        };
        owed.0.finished(connection, message, failed).await
    }
}

/// A message a refused gate fails, from the status it holds.
struct Failing<'a> {
    draft: &'a read::Locked,
    sequence: i64,
    running: bool,
}

impl Failing<'_> {
    async fn fail(
        &self,
        connection: &mut PgConnection,
        reason: FailureReason,
        owed: &mut SetupOwed,
    ) -> Result<()> {
        let failed = MessageState::Failed.as_str();
        sqlx::query(crate::sql::message::FAIL)
            .bind(&self.draft.id)
            .bind(self.sequence)
            .bind(failed)
            .bind(verdict::reason(reason))
            .bind(if self.running { RUNNING } else { QUEUED })
            .execute(&mut *connection)
            .await
            .map_err(query(sql::CONTEXT))?;
        let message = crate::funnel::Message {
            workspace: &self.draft.workspace_id,
            creator: &self.draft.user_id,
            draft: &self.draft.id,
        };
        owed.0.finished(connection, message, failed).await
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

async fn history(
    connection: &mut PgConnection,
    draft: &Uuid7,
    sequence: i64,
) -> Result<Vec<SetupTurnText>> {
    let rows: Vec<(String, String)> = sqlx::query_as(sql::HISTORY)
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
        .map(|(message, answer)| SetupTurnText { message, answer })
        .collect())
}
