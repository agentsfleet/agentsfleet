//! Publication is checked before replay, using database time after every lock wait.

use std::time::Duration;

use crate::{
    Access, Result, access,
    draft::read,
    error::{ErrorKind, raise::query},
    model,
    types::{DraftState, RUNNING},
};
use af_core::id::Uuid7;
use af_wire::authoring::FailureReason;
use afd_credential::provider::RegisteredModel;
use jiff::Timestamp;
use sqlx::PgConnection;

use super::{Status, sql};

mod connected;
mod counters;
pub(crate) use counters::{Counters, DELETION_GENERATION};

const CONTEXT: &str = "authorize authoring run publication";

#[derive(Clone)]
pub(crate) struct Attempt {
    pub(crate) policy: crate::turn_policy::TurnPolicy,
    pub(crate) id: Uuid7,
    pub(crate) workspace_id: Uuid7,
    pub(crate) tenant_id: Uuid7,
    pub(crate) user_id: Uuid7,
    pub(crate) draft_id: Uuid7,
    pub(crate) message_sequence: i64,
    pub(crate) fencing_token: i64,
    pub(crate) model_snapshot: RegisteredModel,
    pub(crate) status: Status,
    pub(crate) run_expires_at: Timestamp,
    /// The attempt's wall-clock deadline; renewal never moves it.
    pub(crate) expires_at: Timestamp,
    pub(crate) last_activity_sequence: i64,
    pub(crate) model_token_hash: String,
    pub(crate) call_count: i32,
    pub(crate) output_token_count: i64,
    pub(crate) usage_unknown: bool,
    pub(crate) reserved_nanos: i64,
    pub(crate) created_at: Timestamp,
    /// The runner's last verified runtime, stored as whole milliseconds.
    pub(crate) reported_runtime: Duration,
    /// The daemon's observed refusal, which outranks any runner report.
    pub(crate) failure_reason: Option<FailureReason>,
}

mod rows;
pub(crate) use rows::AttemptRow;

pub(crate) struct Guarded {
    pub(crate) attempt: Attempt,
    pub(crate) draft: read::Locked,
    pub(crate) now: Timestamp,
}

/// Validates live private authority before checking a model capability or key.
pub(in crate::run) async fn bound(
    connection: &mut PgConnection,
    runner: &Uuid7,
    run: &Uuid7,
    counters: &Counters,
) -> Result<Guarded> {
    let first = attempt(connection, sql::FIND, runner, run).await?;
    let draft = subject(connection, &first).await?;
    let locked = attempt(connection, sql::LOCK, runner, run).await?;
    let running: bool = sqlx::query_scalar(sql::MESSAGE)
        .bind(&draft.id)
        .bind(locked.message_sequence)
        .bind(RUNNING)
        .fetch_one(&mut *connection)
        .await
        .map_err(query(CONTEXT))?;
    if !running {
        return Err(ErrorKind::Fence.into());
    }
    let now = crate::sql::clock(connection, CONTEXT).await?;
    check(&locked, &draft, counters, now, Purpose::Invocation)?;
    Ok(Guarded {
        attempt: locked,
        draft,
        now,
    })
}

pub(crate) async fn current_model(connection: &mut PgConnection, guarded: &Guarded) -> Result<()> {
    let live = model::selected(
        connection,
        &guarded.attempt.tenant_id,
        &guarded.attempt.model_snapshot.reference,
    )
    .await?;
    if live != guarded.attempt.model_snapshot {
        return Err(ErrorKind::Model.into());
    }
    Ok(())
}

/// Accounting needs the original authenticated attempt, not live private access.
pub(crate) async fn accounting(
    connection: &mut PgConnection,
    runner: &Uuid7,
    run: &Uuid7,
    counters: &Counters,
) -> Result<Attempt> {
    let locked = attempt(connection, sql::LOCK, runner, run).await?;
    if !counters.binds(&locked) {
        return Err(ErrorKind::Fence.into());
    }
    Ok(locked)
}

/// Whether Stop cancelled the attempt's turn, or its draft left editing or
/// went, read without requiring access to private content.
pub(crate) async fn cancelled(connection: &mut PgConnection, attempt: &Attempt) -> Result<bool> {
    sqlx::query_scalar(sql::CANCELLED)
        .bind(&attempt.draft_id)
        .bind(attempt.message_sequence)
        .bind(crate::types::EDITING)
        .bind(crate::types::CANCELLED)
        .fetch_one(connection)
        .await
        .map_err(query(CONTEXT))
}

#[derive(Clone, Copy)]
enum Purpose {
    Publication,
    Invocation,
}

impl Purpose {
    fn accepts(self, attempt: &Attempt) -> bool {
        match self {
            Self::Publication => attempt.status == Status::Active && !attempt.usage_unknown,
            Self::Invocation => {
                matches!(attempt.status, Status::Active | Status::AwaitingAccounting)
            }
        }
    }
}

pub(crate) async fn refresh(
    connection: &mut PgConnection,
    guarded: &Guarded,
    counters: &Counters,
) -> Result<Timestamp> {
    let now = crate::sql::clock(connection, CONTEXT).await?;
    check(
        &guarded.attempt,
        &guarded.draft,
        counters,
        now,
        Purpose::Invocation,
    )?;
    Ok(now)
}

async fn subject(connection: &mut PgConnection, attempt: &Attempt) -> Result<read::Locked> {
    let subject: Option<String> = sqlx::query_scalar(crate::sql::access::SUBJECT)
        .bind(&attempt.user_id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(query(CONTEXT))?;
    let subject = subject.ok_or(ErrorKind::Hidden)?;
    let access = Access {
        workspace: &attempt.workspace_id,
        subject: &subject,
    };
    let person = access::current(connection, access).await?;
    if person.id != attempt.user_id || person.owner != attempt.tenant_id {
        return Err(ErrorKind::Hidden.into());
    }
    read::lock(connection, access, &person, &attempt.draft_id).await
}

/// The runner's attempt for `run`, read without a lock: what a caller needs
/// to check its funds with the biller before any transaction opens. The guard
/// inside that transaction re-reads it under the lock and decides.
pub(crate) async fn peek(
    connection: &mut PgConnection,
    runner: &Uuid7,
    run: &Uuid7,
) -> Result<Attempt> {
    attempt(connection, sql::FIND, runner, run).await
}

async fn attempt(
    connection: &mut PgConnection,
    statement: &'static str,
    runner: &Uuid7,
    run: &Uuid7,
) -> Result<Attempt> {
    let row: Option<AttemptRow> = sqlx::query_as(statement)
        .bind(run)
        .bind(runner)
        .fetch_optional(connection)
        .await
        .map_err(query(CONTEXT))?;
    row.ok_or(ErrorKind::Hidden)?.decode()
}

fn check(
    attempt: &Attempt,
    draft: &read::Locked,
    counters: &Counters,
    now: Timestamp,
    purpose: Purpose,
) -> Result<()> {
    if attempt.fencing_token != counters.fence
        || counters
            .sequence
            .is_some_and(|sequence| attempt.message_sequence != sequence)
        || !purpose.accepts(attempt)
        || attempt.failure_reason.is_some()
        || attempt.run_expires_at <= now
        || attempt.expires_at <= now
        || draft.status != DraftState::Editing
    {
        return Err(ErrorKind::Fence.into());
    }
    Ok(())
}
