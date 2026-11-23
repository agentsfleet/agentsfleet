//! Whether a candidate draft's creator may still have its turn run, read
//! without a lock. A hint carries no authority; this read is a hint too, and
//! the thread and turn claims after it decide (`super::sql::RUNNING`).

use af_core::{error_code, id::Uuid7};
use sqlx::PgConnection;

use super::sql;
use crate::{
    Access, Result, access,
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

/// Whether `hint`'s creator still belongs to the draft's workspace. A creator
/// who no longer does has the draft's queued messages cancelled, owing one
/// finish per row moved; one whose account is gone took the draft with it.
pub(super) async fn holds(
    connection: &mut PgConnection,
    hint: &Hint<'_>,
    owed: &mut Owed,
) -> Result<bool> {
    let subject: Option<String> = sqlx::query_scalar(crate::sql::access::SUBJECT)
        .bind(hint.user_id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(query(sql::CONTEXT))?;
    let Some(subject) = subject else {
        return Ok(false);
    };
    let access = Access {
        workspace: hint.workspace_id,
        subject: &subject,
    };
    // The subject is the hint's creator's own, and a subject names one person;
    // the draft's tenant is its workspace's (`fk_drafts_workspace_tenant`).
    match access::unlocked(connection, access).await {
        Ok(_person) => Ok(true),
        Err(error) if error.code() == error_code::SETUP_NOT_FOUND => {
            cancel_revoked(connection, hint, &subject, owed).await?;
            Ok(false)
        }
        Err(error) => Err(error),
    }
}

/// Cancels a revoked creator's queued messages, owing one finish per row
/// moved. One statement that moves only rows still queued, so a message a
/// concurrent claim already took is left to its run.
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
