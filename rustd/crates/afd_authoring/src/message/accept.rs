//! Acceptance pins sequence/model once and checks replay before current state.

use crate::{
    Access, Drafts, Result,
    access::{self, Person},
    draft::read,
    error::{
        ErrorKind,
        raise::{invalid, query},
    },
    lease::pending::{self, Pending},
    model, replay, sql,
    types::{Accepted, DraftState, QUEUED, SendMessage},
};
use afd_core::id::Uuid7;
use afd_observability::AuthoringStep;
use jiff::Timestamp;
use sqlx::{Connection as _, PgConnection};

const CONTEXT: &str = "accept private draft message";
const SEQUENCE_EXHAUSTED: &str = "message sequence is exhausted";

/// The receipt scope a draft's message retry keys are filed under.
pub(crate) fn scope(workspace: &Uuid7, draft: &Uuid7) -> String {
    format!("{workspace}:draft:{draft}:message")
}

impl Drafts {
    /// Accepts one request without promising a runner, balance or model result.
    ///
    /// # Errors
    /// Refuses inaccessible/closed drafts and changed-body replay; no sequence
    /// or user text is written on refusal.
    pub async fn send(
        &self,
        access: Access<'_>,
        id: &Uuid7,
        request: SendMessage,
        now: Timestamp,
    ) -> Result<Accepted> {
        let request = garde::Unvalidated::new(request).validate()?;
        let text = super::normalize(&request.message);
        let digest = replay::digest(&text)?;
        let mut connection = self.database.acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        let person = access::current(&mut tx, access).await?;
        let draft = read::lock(&mut tx, access, &person, id).await?;
        let scope = scope(access.workspace, id);
        let prior = replayed(
            &mut tx,
            &scope,
            &request.operation_id,
            &person.id,
            &digest,
            id,
        )
        .await?;
        let accepted = if let Some(accepted) = prior {
            accepted
        } else {
            self.accept_fresh(&mut tx, &person, &draft, &request, &text, now)
                .await?
        };
        tx.commit().await.map_err(query(CONTEXT))?;
        if !accepted.replayed {
            let draft_id = id.clone();
            let step = AuthoringStep::MessageAccepted { draft_id };
            self.reached(access, step);
        }
        Ok(accepted)
    }

    async fn accept_fresh(
        &self,
        connection: &mut PgConnection,
        person: &Person,
        draft: &read::Locked,
        request: &SendMessage,
        text: &str,
        now: Timestamp,
    ) -> Result<Accepted> {
        if draft.status != DraftState::Editing {
            return Err(ErrorKind::State {
                current: draft.status.as_str(),
            }
            .into());
        }
        if pending::current(connection, draft).await? == Some(Pending::UsageUnknown) {
            return Err(ErrorKind::Unknown.into());
        }
        let sequence = draft
            .last_message_sequence
            .checked_add(1)
            .ok_or_else(|| invalid(SEQUENCE_EXHAUSTED))?;
        insert(
            connection,
            Insertion {
                id: &self.mint(now)?,
                receipt: &self.mint(now)?,
                workspace: &draft.workspace_id,
                person,
                draft: &draft.id,
                sequence,
                operation: &request.operation_id,
                text,
                model: &draft.authoring_model_ref,
            },
        )
        .await?;
        Ok(Accepted {
            status: super::ACCEPTED,
            sequence: sequence.to_string(),
            replayed: false,
        })
    }
}

async fn replayed(
    connection: &mut PgConnection,
    scope: &str,
    operation: &Uuid7,
    person: &Uuid7,
    digest: &str,
    draft: &Uuid7,
) -> Result<Option<Accepted>> {
    let Some(message) = replay::find(
        connection,
        scope,
        replay::Resource::Message,
        operation,
        person,
        digest,
    )
    .await?
    else {
        return Ok(None);
    };
    let sequence: Option<i64> = sqlx::query_scalar(sql::message::SEQUENCE)
        .bind(message.as_str())
        .bind(draft.as_str())
        .fetch_optional(connection)
        .await
        .map_err(query(CONTEXT))?;
    Ok(Some(Accepted {
        status: super::ACCEPTED,
        sequence: sequence.ok_or(ErrorKind::Gone)?.to_string(),
        replayed: true,
    }))
}

pub(crate) struct Insertion<'a> {
    pub(crate) id: &'a Uuid7,
    /// The message's retry receipt.
    pub(crate) receipt: &'a Uuid7,
    pub(crate) workspace: &'a Uuid7,
    pub(crate) person: &'a Person,
    pub(crate) draft: &'a Uuid7,
    pub(crate) sequence: i64,
    pub(crate) operation: &'a Uuid7,
    pub(crate) text: &'a str,
    pub(crate) model: &'a str,
}

/// Writes the message, files its retry key and advances the draft's sequence.
///
/// The caller holds the draft lock, so no concurrent request files the same
/// key: a reservation that loses is a replay the caller already missed.
pub(crate) async fn insert(connection: &mut PgConnection, insertion: Insertion<'_>) -> Result<()> {
    sqlx::query(sql::message::INSERT)
        .bind(insertion.id.as_str())
        .bind(insertion.draft.as_str())
        .bind(insertion.sequence)
        .bind(insertion.operation.as_str())
        .bind(QUEUED)
        .bind(model::tenant_model(insertion.model))
        .bind(insertion.text)
        .execute(&mut *connection)
        .await
        .map_err(query(CONTEXT))?;
    let reserved = replay::reserve(
        &mut *connection,
        replay::Reservation {
            receipt_id: insertion.receipt,
            workspace: insertion.workspace,
            person: insertion.person,
            scope: &scope(insertion.workspace, insertion.draft),
            operation: insertion.operation,
            digest: &replay::digest(&insertion.text)?,
            resource: insertion.id,
        },
    )
    .await?;
    if !reserved {
        return Err(ErrorKind::Replay.into());
    }
    sqlx::query(sql::message::ADVANCE)
        .bind(insertion.draft.as_str())
        .bind(insertion.sequence)
        .execute(connection)
        .await
        .map_err(query(CONTEXT))?;
    Ok(())
}
