//! Wallet and current subject precede the draft lock; queue hints carry no authority.

use afd_core::{error_code, id::Uuid7};
use afd_db::Db;
use sqlx::PgConnection;

use super::sql;
use crate::{
    Access, Result, access,
    draft::read,
    error::raise::query,
    funnel::{Message, Owed},
    lease::Status,
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
    let outstanding: bool = sqlx::query_scalar(sql::OBLIGATION)
        .bind(draft.id.as_str())
        .bind(Status::Settled.as_str())
        .fetch_one(&mut *connection)
        .await
        .map_err(query(sql::CONTEXT))?;
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
