//! Publication is checked before replay, using database time after every lock wait.

use crate::{
    Access, Result, access,
    draft::{read, value},
    error::{ErrorKind, raise::query},
    model,
    types::{DraftState, RUNNING},
};
use afd_core::{clock::UnixMillis, id::Uuid7};
use afd_credential::provider::RegisteredModel;
use serde::Deserialize;
use sqlx::PgConnection;

use super::{Status, sql};

mod counters;
pub(crate) use counters::Counters;

const CONTEXT: &str = "authorize authoring lease publication";

#[derive(Clone, Deserialize)]
pub(crate) struct Attempt {
    pub(crate) id: Uuid7,
    pub(crate) workspace_id: Uuid7,
    pub(crate) tenant_id: Uuid7,
    pub(crate) creator_id: Uuid7,
    pub(crate) draft_id: Uuid7,
    pub(crate) message_sequence: i64,
    pub(crate) fence: i64,
    pub(crate) deletion_generation: i64,
    pub(crate) model_snapshot: RegisteredModel,
    pub(crate) status: Status,
    pub(crate) lease_expires_at: i64,
    pub(crate) deadline: i64,
    pub(crate) activity_sequence: i64,
    pub(crate) capability_hash: String,
    pub(crate) calls: i32,
    pub(crate) output_tokens: i64,
    pub(crate) usage_unknown: bool,
    pub(crate) reserved_nanos: i64,
    pub(crate) created_at: i64,
    pub(crate) reported_runtime_ms: i64,
    pub(crate) daemon_failure: Option<afd_wire::authoring::FailureReason>,
}

pub(crate) struct Guarded {
    pub(crate) attempt: Attempt,
    pub(crate) draft: read::Locked,
    pub(crate) now: UnixMillis,
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
    let now: i64 = sqlx::query_scalar(sql::CLOCK)
        .fetch_one(connection)
        .await
        .map_err(query(CONTEXT))?;
    check(&locked, &draft, counters, now, Purpose::Invocation)?;
    Ok(Guarded {
        attempt: locked,
        draft,
        now: UnixMillis::from_millis(now),
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
    const CANCELLED: &str = "\
SELECT NOT EXISTS(SELECT 1 FROM core.authoring_drafts WHERE id=$1::uuid)
OR EXISTS(SELECT 1 FROM core.authoring_drafts WHERE id=$1::uuid
AND (cancelled_through >= $2 OR deletion_generation > $3 OR state != $4))";
    sqlx::query_scalar(CANCELLED)
        .bind(attempt.draft_id.as_str())
        .bind(attempt.message_sequence)
        .bind(attempt.deletion_generation)
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
) -> Result<UnixMillis> {
    let now: i64 = sqlx::query_scalar(sql::CLOCK)
        .fetch_one(connection)
        .await
        .map_err(query(CONTEXT))?;
    check(
        &guarded.attempt,
        &guarded.draft,
        counters,
        now,
        Purpose::Invocation,
    )?;
    Ok(UnixMillis::from_millis(now))
}

async fn subject(connection: &mut PgConnection, attempt: &Attempt) -> Result<read::Locked> {
    let _wallet: Option<String> = sqlx::query_scalar(sql::WALLET)
        .bind(attempt.tenant_id.as_str())
        .fetch_optional(&mut *connection)
        .await
        .map_err(query(CONTEXT))?;
    let subject: Option<String> = sqlx::query_scalar(sql::SUBJECT)
        .bind(attempt.creator_id.as_str())
        .fetch_optional(&mut *connection)
        .await
        .map_err(query(CONTEXT))?;
    let subject = subject.ok_or(ErrorKind::Hidden)?;
    let access = Access {
        workspace: &attempt.workspace_id,
        subject: &subject,
    };
    let person = access::current(connection, access).await?;
    if person.id != attempt.creator_id || person.owner != attempt.tenant_id {
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
    let raw: Option<String> = sqlx::query_scalar(statement)
        .bind(lease.as_str())
        .bind(runner.as_str())
        .fetch_optional(connection)
        .await
        .map_err(query(CONTEXT))?;
    value(&raw.ok_or(ErrorKind::Hidden)?)
}

fn check(
    attempt: &Attempt,
    draft: &read::Locked,
    counters: &Counters,
    now: i64,
    purpose: Purpose,
) -> Result<()> {
    if attempt.fence != counters.fence
        || attempt.deletion_generation != counters.generation
        || attempt.deletion_generation != draft.deletion_generation
        || counters
            .sequence
            .is_some_and(|sequence| attempt.message_sequence != sequence)
        || attempt.message_sequence <= draft.cancelled_through
        || !purpose.accepts(attempt)
        || attempt.daemon_failure.is_some()
        || attempt.lease_expires_at <= now
        || attempt.deadline <= now
        || draft.state != DraftState::Editing
    {
        return Err(ErrorKind::Fence.into());
    }
    Ok(())
}
