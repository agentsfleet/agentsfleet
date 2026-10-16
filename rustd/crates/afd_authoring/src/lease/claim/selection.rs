//! Wallet and current subject precede the draft lock; queue hints carry no authority.

use afd_core::{error_code, id::Uuid7};
use afd_db::Db;
use serde::Deserialize;
use sqlx::PgConnection;

use super::sql;
use crate::{
    Access, Result, access,
    draft::read,
    error::raise::query,
    funnel::{Message, Owed},
    lease::Status,
    types::{CANCELLED, DraftState, EDITING, QUEUED},
};

#[derive(Deserialize)]
pub(super) struct Hint {
    pub(super) id: Uuid7,
    pub(super) workspace_id: Uuid7,
    pub(super) tenant_id: Uuid7,
    pub(super) creator_id: Uuid7,
}

pub(super) struct Selected {
    pub(super) draft: read::Locked,
    pub(super) sequence: i64,
    pub(super) model_ref: String,
}

pub(super) async fn candidates(database: &Db) -> Result<Vec<Hint>> {
    let rows: Vec<(String, String, String, String)> = sqlx::query_as(sql::CANDIDATES)
        .bind(EDITING)
        .bind(QUEUED)
        .bind(Status::Settled.as_str())
        .bind(sql::SCAN_LIMIT)
        .fetch_all(&mut *database.acquire().await?)
        .await
        .map_err(query(sql::CONTEXT))?;
    rows.into_iter()
        .map(|(id, workspace, tenant, creator)| {
            Ok(Hint {
                id: Uuid7::parse(&id)?,
                workspace_id: Uuid7::parse(&workspace)?,
                tenant_id: Uuid7::parse(&tenant)?,
                creator_id: Uuid7::parse(&creator)?,
            })
        })
        .collect()
}

pub(super) async fn authorize(
    connection: &mut PgConnection,
    hint: &Hint,
    owed: &mut Owed,
) -> Result<Option<Selected>> {
    let _wallet: Option<String> = sqlx::query_scalar(crate::lease::sql::WALLET)
        .bind(hint.tenant_id.as_str())
        .fetch_optional(&mut *connection)
        .await
        .map_err(query(sql::CONTEXT))?;
    let Some(subject) = subject(connection, hint).await? else {
        return Ok(None);
    };
    let access = Access {
        workspace: &hint.workspace_id,
        subject: &subject,
    };
    let person = access::current(connection, access).await;
    let person = match person {
        Ok(person) if person.id == hint.creator_id && person.owner == hint.tenant_id => person,
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
    sqlx::query_scalar(crate::lease::sql::SUBJECT)
        .bind(hint.creator_id.as_str())
        .fetch_optional(connection)
        .await
        .map_err(query(sql::CONTEXT))
}

async fn select_message(
    connection: &mut PgConnection,
    draft: read::Locked,
) -> Result<Option<Selected>> {
    let outstanding: bool = sqlx::query_scalar(sql::OBLIGATION)
        .bind(draft.id.as_str())
        .bind(Status::Settled.as_str())
        .fetch_one(&mut *connection)
        .await
        .map_err(query(sql::CONTEXT))?;
    if draft.state != DraftState::Editing || outstanding {
        return Ok(None);
    }
    let next: Option<(i64, String)> = sqlx::query_as(sql::MESSAGE)
        .bind(draft.id.as_str())
        .bind(QUEUED)
        .bind(draft.cancelled_through)
        .fetch_optional(connection)
        .await
        .map_err(query(sql::CONTEXT))?;
    Ok(next.map(|(sequence, model_ref)| Selected {
        draft,
        sequence,
        model_ref,
    }))
}

/// Cancels a revoked creator's queued messages, owing one finish per row moved.
async fn cancel_revoked(
    connection: &mut PgConnection,
    hint: &Hint,
    subject: &str,
    owed: &mut Owed,
) -> Result<()> {
    const CANCEL: &str = "\
UPDATE core.authoring_messages m SET state=$5,finished_at=$6,updated_at=$6
FROM core.authoring_drafts d WHERE m.draft_id=d.id AND d.id=$1::uuid
AND d.workspace_id=$2::uuid AND d.tenant_id=$3::uuid AND d.creator_id=$4::uuid
AND m.state=$7";
    let now: i64 = sqlx::query_scalar(crate::sql::CLOCK)
        .fetch_one(&mut *connection)
        .await
        .map_err(query(sql::CONTEXT))?;
    let cancelled = sqlx::query(CANCEL)
        .bind(hint.id.as_str())
        .bind(hint.workspace_id.as_str())
        .bind(hint.tenant_id.as_str())
        .bind(hint.creator_id.as_str())
        .bind(CANCELLED)
        .bind(now)
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
            creator: &hint.creator_id,
            draft: &hint.id,
        }
    }
}

#[cfg(feature = "test-util")]
pub(super) async fn hint(connection: &mut PgConnection, draft: &Uuid7) -> Result<Hint> {
    const READ: &str = "SELECT to_jsonb(d)::text FROM core.authoring_drafts d WHERE id=$1::uuid";
    let raw: String = sqlx::query_scalar(READ)
        .bind(draft.as_str())
        .fetch_one(connection)
        .await
        .map_err(query(sql::CONTEXT))?;
    crate::draft::value(&raw)
}
