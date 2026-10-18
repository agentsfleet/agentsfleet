//! Publication is checked before replay, using database time after every lock wait.

use std::time::Duration;

use crate::{
    Access, Result, access,
    draft::{read, value, word},
    error::{ErrorKind, raise::query},
    model,
    types::{DraftState, RUNNING},
};
use afd_core::id::Uuid7;
use afd_credential::provider::RegisteredModel;
use afd_wire::authoring::FailureReason;
use jiff::Timestamp;
use sqlx::PgConnection;

use super::{Status, sql};

mod counters;
pub(crate) use counters::{Counters, DELETION_GENERATION};

const CONTEXT: &str = "authorize authoring lease publication";

#[derive(Clone)]
pub(crate) struct Attempt {
    pub(crate) id: Uuid7,
    pub(crate) workspace_id: Uuid7,
    pub(crate) tenant_id: Uuid7,
    pub(crate) user_id: Uuid7,
    pub(crate) draft_id: Uuid7,
    pub(crate) message_sequence: i64,
    pub(crate) fencing_token: i64,
    pub(crate) model_snapshot: RegisteredModel,
    pub(crate) status: Status,
    pub(crate) lease_expires_at: Timestamp,
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

/// One `authoring.attempts` row, by column name.
#[derive(sqlx::FromRow)]
pub(crate) struct AttemptRow {
    id: String,
    workspace_id: String,
    tenant_id: String,
    user_id: String,
    draft_id: String,
    message_sequence: i64,
    fencing_token: i64,
    model_snapshot: String,
    status: String,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    lease_expires_at: Timestamp,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    expires_at: Timestamp,
    last_activity_sequence: i64,
    model_token_hash: String,
    call_count: i32,
    output_token_count: i64,
    usage_unknown: bool,
    reserved_nanos: i64,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    created_at: Timestamp,
    reported_runtime_ms: i64,
    failure_reason: Option<String>,
}

impl AttemptRow {
    pub(crate) fn decode(self) -> Result<Attempt> {
        Ok(Attempt {
            id: Uuid7::parse(&self.id)?,
            workspace_id: Uuid7::parse(&self.workspace_id)?,
            tenant_id: Uuid7::parse(&self.tenant_id)?,
            user_id: Uuid7::parse(&self.user_id)?,
            draft_id: Uuid7::parse(&self.draft_id)?,
            message_sequence: self.message_sequence,
            fencing_token: self.fencing_token,
            model_snapshot: value(&self.model_snapshot)?,
            status: word(&self.status)?,
            lease_expires_at: self.lease_expires_at,
            expires_at: self.expires_at,
            last_activity_sequence: self.last_activity_sequence,
            model_token_hash: self.model_token_hash,
            call_count: self.call_count,
            output_token_count: self.output_token_count,
            usage_unknown: self.usage_unknown,
            reserved_nanos: self.reserved_nanos,
            created_at: self.created_at,
            reported_runtime: Duration::from_millis(
                u64::try_from(self.reported_runtime_ms).unwrap_or(0),
            ),
            failure_reason: self.failure_reason.as_deref().map(word).transpose()?,
        })
    }
}

pub(crate) struct Guarded {
    pub(crate) attempt: Attempt,
    pub(crate) draft: read::Locked,
    pub(crate) now: Timestamp,
}

pub(crate) async fn publication(
    connection: &mut PgConnection,
    runner: &Uuid7,
    lease: &Uuid7,
    counters: &Counters,
) -> Result<Guarded> {
    authorize(connection, runner, lease, counters, Purpose::Publication).await
}

/// Validates live private authority before checking a model capability or key.
pub(crate) async fn bound(
    connection: &mut PgConnection,
    runner: &Uuid7,
    lease: &Uuid7,
    counters: &Counters,
) -> Result<Guarded> {
    let first = attempt(connection, sql::FIND, runner, lease).await?;
    let draft = subject(connection, &first).await?;
    let locked = attempt(connection, sql::LOCK, runner, lease).await?;
    let running: bool = sqlx::query_scalar(sql::MESSAGE)
        .bind(draft.id.as_str())
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
    lease: &Uuid7,
    counters: &Counters,
) -> Result<Attempt> {
    let first = attempt(connection, sql::FIND, runner, lease).await?;
    let _wallet: Option<String> = sqlx::query_scalar(sql::WALLET)
        .bind(first.tenant_id.as_str())
        .fetch_optional(&mut *connection)
        .await
        .map_err(query(CONTEXT))?;
    let locked = attempt(connection, sql::LOCK, runner, lease).await?;
    if !counters.binds(&locked) {
        return Err(ErrorKind::Fence.into());
    }
    Ok(locked)
}

/// Reads a saved cancellation fence without requiring access to private content.
pub(crate) async fn cancelled(connection: &mut PgConnection, attempt: &Attempt) -> Result<bool> {
    sqlx::query_scalar(sql::CANCELLED)
        .bind(attempt.draft_id.as_str())
        .bind(attempt.message_sequence)
        .bind(crate::types::EDITING)
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

async fn authorize(
    connection: &mut PgConnection,
    runner: &Uuid7,
    lease: &Uuid7,
    counters: &Counters,
    purpose: Purpose,
) -> Result<Guarded> {
    let guarded = bound(connection, runner, lease, counters).await?;
    if !purpose.accepts(&guarded.attempt) {
        return Err(ErrorKind::Fence.into());
    }
    current_model(connection, &guarded).await?;
    Ok(guarded)
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
    let _wallet: Option<String> = sqlx::query_scalar(sql::WALLET)
        .bind(attempt.tenant_id.as_str())
        .fetch_optional(&mut *connection)
        .await
        .map_err(query(CONTEXT))?;
    let subject: Option<String> = sqlx::query_scalar(crate::sql::access::SUBJECT)
        .bind(attempt.user_id.as_str())
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

async fn attempt(
    connection: &mut PgConnection,
    statement: &'static str,
    runner: &Uuid7,
    lease: &Uuid7,
) -> Result<Attempt> {
    let row: Option<AttemptRow> = sqlx::query_as(statement)
        .bind(lease.as_str())
        .bind(runner.as_str())
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
        || counters.generation != DELETION_GENERATION
        || counters
            .sequence
            .is_some_and(|sequence| attempt.message_sequence != sequence)
        || attempt.message_sequence <= draft.cancelled_through_sequence
        || !purpose.accepts(attempt)
        || attempt.failure_reason.is_some()
        || attempt.lease_expires_at <= now
        || attempt.expires_at <= now
        || draft.status != DraftState::Editing
    {
        return Err(ErrorKind::Fence.into());
    }
    Ok(())
}
