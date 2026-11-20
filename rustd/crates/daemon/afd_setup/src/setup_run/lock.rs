//! Taking a candidate draft for an assignment: its creator first, then the
//! draft, neither waited for. A hint carries no authority; what this finds
//! under the locks does.

use af_core::{error_code, id::Uuid7};
use sqlx::{Connection as _, PgConnection};

use super::sql;
use crate::{
    Access, Result, access,
    draft::read,
    error::raise::query,
    funnel::{Message, Owed},
    types::{CANCELLED, QUEUED},
};

/// A draft an assignment may try, as a hint read without a lock.
pub(super) struct Hint<'a> {
    pub(super) id: &'a Uuid7,
    pub(super) workspace_id: &'a Uuid7,
    pub(super) tenant_id: &'a Uuid7,
    pub(super) user_id: &'a Uuid7,
}

/// What taking one candidate's creator and draft found, before any message is
/// chosen.
pub(super) enum DraftLock {
    /// The creator still holds the draft, which is locked for this assignment.
    Locked(Box<read::Locked>),
    /// Nothing in this candidate may be assigned now.
    Passed,
    /// Another transaction holds its creator or its draft.
    Busy,
}

/// Takes the creator's row and the draft's without waiting, proves the
/// creator still holds the draft, and locks it; a revoked creator's queued
/// messages are cancelled on the way.
pub(super) async fn lock_draft(
    connection: &mut PgConnection,
    hint: &Hint<'_>,
    owed: &mut Owed,
) -> Result<DraftLock> {
    if !held(connection, hint).await? {
        return Ok(DraftLock::Busy);
    }
    Ok(selected(connection, hint, owed)
        .await?
        .map_or(DraftLock::Passed, |draft| {
            DraftLock::Locked(Box::new(draft))
        }))
}

/// Whether the draft's thread holds a live run, read under the draft's lock:
/// one live run per draft, and its thread's id is the draft's.
pub(super) async fn running(connection: &mut PgConnection, draft: &Uuid7) -> Result<bool> {
    sqlx::query_scalar(sql::LIVE)
        .bind(draft)
        .fetch_one(connection)
        .await
        .map_err(query(sql::CONTEXT))
}

/// Take the creator's row and then the draft's without waiting, in the order
/// the reads after it lock them, answering whether both were taken.
///
/// In a savepoint, because a refused `NOWAIT` aborts the statement's
/// transaction: rolling the savepoint back keeps the assignment's own
/// transaction, and its host lock, usable for the next candidate. A released
/// savepoint keeps its row locks until the assignment commits. A row that is
/// gone is not held by anyone; the reads after it refuse the candidate.
async fn held(connection: &mut PgConnection, hint: &Hint<'_>) -> Result<bool> {
    let mut savepoint = connection.begin().await.map_err(query(sql::CONTEXT))?;
    for (statement, id) in [(sql::HOLD_USER, hint.user_id), (sql::HOLD_DRAFT, hint.id)] {
        let taken = sqlx::query(statement)
            .bind(id)
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
    hint: &Hint<'_>,
    owed: &mut Owed,
) -> Result<Option<read::Locked>> {
    let subject: Option<String> = sqlx::query_scalar(crate::sql::access::SUBJECT)
        .bind(hint.user_id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(query(sql::CONTEXT))?;
    let Some(subject) = subject else {
        return Ok(None);
    };
    let access = Access {
        workspace: hint.workspace_id,
        subject: &subject,
    };
    let person = match access::current(connection, access).await {
        Ok(person) if &person.id == hint.user_id && &person.owner == hint.tenant_id => person,
        Ok(_) => return Ok(None),
        Err(error) if error.code() == error_code::AUTHORING_NOT_FOUND => {
            cancel_revoked(connection, hint, &subject, owed).await?;
            return Ok(None);
        }
        Err(error) => return Err(error),
    };
    match read::lock(connection, access, &person, hint.id).await {
        Ok(locked) => Ok(Some(locked)),
        Err(error) if error.code() == error_code::AUTHORING_NOT_FOUND => Ok(None),
        Err(error) => Err(error),
    }
}

/// Cancels a revoked creator's queued messages, owing one finish per row moved.
async fn cancel_revoked(
    connection: &mut PgConnection,
    hint: &Hint<'_>,
    subject: &str,
    owed: &mut Owed,
) -> Result<()> {
    let cancelled = sqlx::query(sql::CANCEL_REVOKED)
        .bind(hint.id)
        .bind(hint.workspace_id)
        .bind(hint.tenant_id)
        .bind(hint.user_id)
        .bind(CANCELLED)
        .bind(QUEUED)
        .execute(connection)
        .await
        .map_err(query(sql::CONTEXT))?;
    let message = Message {
        workspace: hint.workspace_id,
        creator: hint.user_id,
        draft: hint.id,
    };
    for _ in 0..cancelled.rows_affected() {
        owed.finished_by(subject.to_owned(), message, CANCELLED);
    }
    Ok(())
}
