//! The creator precedes the draft lock, neither is waited for, and queue hints carry
//! no authority.

use afd_core::{error_code, id::Uuid7};
use afd_db::Db;
use sqlx::{Connection as _, PgConnection};

use super::sql;
use crate::{
    Access, Result, access,
    draft::read,
    error::raise::query,
    funnel::{Message, Owed},
    lease::{Status, pending},
    model,
    types::{CANCELLED, DraftState, EDITING, QUEUED},
};

pub(super) struct Hint {
    pub(super) id: Uuid7,
    pub(super) workspace_id: Uuid7,
    pub(super) tenant_id: Uuid7,
    pub(super) user_id: Uuid7,
}

/// [`sql::CANDIDATES`]'s row, by column name.
#[derive(sqlx::FromRow)]
struct HintRow {
    id: String,
    workspace_id: String,
    tenant_id: String,
    user_id: String,
}

impl HintRow {
    fn decode(self) -> Result<Hint> {
        Ok(Hint {
            id: Uuid7::parse(&self.id)?,
            workspace_id: Uuid7::parse(&self.workspace_id)?,
            tenant_id: Uuid7::parse(&self.tenant_id)?,
            user_id: Uuid7::parse(&self.user_id)?,
        })
    }
}

/// [`sql::MESSAGE`]'s row, by column name.
#[derive(sqlx::FromRow)]
struct Next {
    sequence: i64,
    tenant_model_id: Option<String>,
}

/// What authorizing one candidate found.
pub(super) enum Authorized {
    /// The draft and its next queued message, locked for admission.
    Selected(Box<Selected>),
    /// Nothing this candidate may admit now.
    Passed,
    /// Another transaction holds its creator or its draft; skipped this poll
    /// rather than waited on, so one busy draft never stalls the walk.
    Locked,
}

pub(super) struct Selected {
    pub(super) draft: read::Locked,
    pub(super) sequence: i64,
    pub(super) model_ref: String,
}

pub(super) async fn candidates(database: &Db) -> Result<Vec<Hint>> {
    let rows: Vec<HintRow> = sqlx::query_as(sql::CANDIDATES)
        .bind(EDITING)
        .bind(QUEUED)
        .bind(Status::Settled.as_str())
        .bind(sql::SCAN_LIMIT)
        .fetch_all(&mut *database.acquire().await?)
        .await
        .map_err(query(sql::CONTEXT))?;
    rows.into_iter().map(HintRow::decode).collect()
}

pub(super) async fn authorize(
    connection: &mut PgConnection,
    hint: &Hint,
    owed: &mut Owed,
) -> Result<Authorized> {
    if !held(connection, hint).await? {
        return Ok(Authorized::Locked);
    }
    Ok(match selected(connection, hint, owed).await? {
        Some(selected) => Authorized::Selected(Box::new(selected)),
        None => Authorized::Passed,
    })
}

/// Take the creator's row and then the draft's without waiting, in the order
/// the reads after it lock them, answering whether both were taken.
///
/// In a savepoint, because a refused `NOWAIT` aborts the statement's
/// transaction: rolling the savepoint back keeps the claim's own transaction,
/// and its host lock, usable for the next candidate. A released savepoint
/// keeps its row locks until the claim commits. A row that is gone is not
/// held by anyone; the reads after it refuse the candidate.
async fn held(connection: &mut PgConnection, hint: &Hint) -> Result<bool> {
    let mut savepoint = connection.begin().await.map_err(query(sql::CONTEXT))?;
    for (statement, id) in [(sql::HOLD_USER, &hint.user_id), (sql::HOLD_DRAFT, &hint.id)] {
        let taken = sqlx::query(statement)
            .bind(id.as_str())
            .fetch_optional(&mut *savepoint)
            .await;
        match taken {
            Ok(_row) => {}
            Err(error) if locked(&error) => {
                savepoint.rollback().await.map_err(query(sql::CONTEXT))?;
                return Ok(false);
            }
            Err(error) => return Err(query(sql::CONTEXT)(error)),
        }
    }
    savepoint.commit().await.map_err(query(sql::CONTEXT))?;
    Ok(true)
}

/// Whether `error` is a `NOWAIT` lock that found its row held.
fn locked(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .and_then(sqlx::error::DatabaseError::code)
        .is_some_and(|code| code == sql::LOCK_NOT_AVAILABLE)
}

async fn selected(
    connection: &mut PgConnection,
    hint: &Hint,
    owed: &mut Owed,
) -> Result<Option<Selected>> {
    let Some(subject) = subject(connection, hint).await? else {
        return Ok(None);
    };
    let access = Access {
        workspace: &hint.workspace_id,
        subject: &subject,
    };
    let person = access::current(connection, access).await;
    let person = match person {
        Ok(person) if person.id == hint.user_id && person.owner == hint.tenant_id => person,
        Ok(_) => return Ok(None),
        Err(error) if error.code() == error_code::AUTHORING_NOT_FOUND => {
            cancel_revoked(connection, hint, &subject, owed).await?;
            return Ok(None);
        }
        Err(error) => return Err(error),
    };
    let locked = match read::lock(connection, access, &person, &hint.id).await {
        Ok(locked) => locked,
        Err(error) if error.code() == error_code::AUTHORING_NOT_FOUND => return Ok(None),
        Err(error) => return Err(error),
    };
    select_message(connection, locked).await
}

async fn subject(connection: &mut PgConnection, hint: &Hint) -> Result<Option<String>> {
    sqlx::query_scalar(crate::sql::access::SUBJECT)
        .bind(hint.user_id.as_str())
        .fetch_optional(connection)
        .await
        .map_err(query(sql::CONTEXT))
}

async fn select_message(
    connection: &mut PgConnection,
    draft: read::Locked,
) -> Result<Option<Selected>> {
    let outstanding = pending::current(connection, &draft).await?.is_some();
    if draft.status != DraftState::Editing || outstanding {
        return Ok(None);
    }
    let next: Option<Next> = sqlx::query_as(sql::MESSAGE)
        .bind(draft.id.as_str())
        .bind(QUEUED)
        .bind(draft.cancelled_through_sequence)
        .fetch_optional(connection)
        .await
        .map_err(query(sql::CONTEXT))?;
    Ok(next.map(|next| Selected {
        draft,
        sequence: next.sequence,
        model_ref: model::reference(next.tenant_model_id),
    }))
}

/// Cancels a revoked creator's queued messages, owing one finish per row moved.
async fn cancel_revoked(
    connection: &mut PgConnection,
    hint: &Hint,
    subject: &str,
    owed: &mut Owed,
) -> Result<()> {
    let cancelled = sqlx::query(sql::CANCEL_REVOKED)
        .bind(hint.id.as_str())
        .bind(hint.workspace_id.as_str())
        .bind(hint.tenant_id.as_str())
        .bind(hint.user_id.as_str())
        .bind(CANCELLED)
        .bind(QUEUED)
        .execute(connection)
        .await
        .map_err(query(sql::CONTEXT))?;
    for _ in 0..cancelled.rows_affected() {
        owed.finished_by(subject.to_owned(), hint.into(), CANCELLED);
    }
    Ok(())
}

impl<'a> From<&'a Hint> for Message<'a> {
    fn from(hint: &'a Hint) -> Self {
        Self {
            workspace: &hint.workspace_id,
            creator: &hint.user_id,
            draft: &hint.id,
        }
    }
}

#[cfg(feature = "test-util")]
pub(super) async fn hint(connection: &mut PgConnection, draft: &Uuid7) -> Result<Hint> {
    let row: HintRow = sqlx::query_as(crate::sql::draft::HINT)
        .bind(draft.as_str())
        .fetch_one(connection)
        .await
        .map_err(query(sql::CONTEXT))?;
    row.decode()
}
