//! Acceptance pins sequence/model once and checks replay before current state.

use crate::{
    Access, Drafts, Result,
    access::{self, Person},
    draft::read,
    error::{
        ErrorKind,
        raise::{invalid, query},
    },
    lease::Status,
    model, replay, sql,
    types::{Accepted, DraftState, QUEUED, SendMessage},
};
use afd_core::id::Uuid7;
use afd_observability::AuthoringStep;
use jiff::Timestamp;
use sqlx::{Connection as _, PgConnection};
use std::sync::Arc;
use tokio::sync::Mutex;

const CONTEXT: &str = "accept private draft message";
const SEQUENCE_EXHAUSTED: &str = "message sequence is exhausted";

/// Runs after the draft lock, so its snapshot sees the
/// preceding sender's committed receipt, sequence and Stop watermark.
/// `$1` draft, `$2` next sequence, `$3` message, `$4` operation, `$5` status,
/// `$6` model, `$7` text, `$8` receipt, `$9` workspace, `$10` actor, `$11` scope,
/// `$12` digest, `$13` replay window, `$14` Stop watermark, `$15` settled status.
const ADMIT: &str = include_str!("../sql/message/admit.sql");

#[derive(sqlx::FromRow)]
struct Admission {
    prior: Option<sqlx::types::Json<replay::Receipt>>,
    prior_sequence: Option<i64>,
    usage_unknown: bool,
    sequence: Option<i64>,
}

impl Admission {
    fn accepted(
        self,
        person: &Uuid7,
        digest: &str,
        next: Option<i64>,
        identifiers: Result<(Uuid7, Uuid7)>,
    ) -> Result<Accepted> {
        if replay::decide(self.prior.map(|prior| prior.0), person, digest)?.is_some() {
            return Ok(Accepted {
                status: super::ACCEPTED,
                sequence: self.prior_sequence.ok_or(ErrorKind::Gone)?.to_string(),
                replayed: true,
            });
        }
        if self.usage_unknown {
            return Err(ErrorKind::Unknown.into());
        }
        next.ok_or_else(|| invalid(SEQUENCE_EXHAUSTED))?;
        identifiers?;
        Ok(Accepted {
            status: super::ACCEPTED,
            sequence: self.sequence.ok_or(ErrorKind::Replay)?.to_string(),
            replayed: false,
        })
    }
}

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
        // Keep local row-lock waiters out of the pool. Eviction or another
        // daemon can use a different gate; the transaction still locks the row.
        let gate = self
            .senders
            .get_with(id.clone(), async { Arc::new(Mutex::new(())) })
            .await;
        let _turn = gate.lock_owned().await;
        let mut connection = self.database.acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        let person = access::current(&mut tx, access).await?;
        let draft = read::lock(&mut tx, access, &person, id).await?;
        let accepted = if draft.status == DraftState::Editing {
            self.admit_message(&mut tx, &person, &draft, &request, &text, now)
                .await?
        } else {
            // Closed drafts only answer an existing receipt; they never admit work.
            replayed(
                &mut tx,
                &scope(access.workspace, id),
                &request.operation_id,
                &person.id,
                &message_digest(&text, request.save)?,
                id,
            )
            .await?
            .ok_or(ErrorKind::State {
                current: draft.status.as_str(),
            })?
        };
        tx.commit().await.map_err(query(CONTEXT))?;
        if !accepted.replayed {
            let draft_id = id.clone();
            let step = AuthoringStep::MessageAccepted { draft_id };
            self.reached(access, step);
        }
        Ok(accepted)
    }

    async fn admit_message(
        &self,
        connection: &mut PgConnection,
        person: &Person,
        draft: &read::Locked,
        request: &SendMessage,
        text: &str,
        now: Timestamp,
    ) -> Result<Accepted> {
        let sequence = draft.last_message_sequence.checked_add(1);
        // A replay or refusal still wins over an entropy failure. Missing IDs
        // make the SQL read-only; only a fresh, otherwise valid Send returns it.
        let identifiers = self
            .mint(now)
            .and_then(|message| self.mint(now).map(|receipt| (message, receipt)));
        let ids = identifiers.as_ref().ok();
        let digest = message_digest(text, request.save)?;
        let admitted: Admission = sqlx::query_as(ADMIT)
            .bind(draft.id.as_str())
            .bind(sequence)
            .bind(ids.map(|(message, _)| message.as_str()))
            .bind(request.operation_id.as_str())
            .bind(QUEUED)
            .bind(model::tenant_model(&draft.authoring_model_ref))
            .bind(text)
            .bind(ids.map(|(_, receipt)| receipt.as_str()))
            .bind(draft.workspace_id.as_str())
            .bind(person.id.as_str())
            .bind(scope(&draft.workspace_id, &draft.id))
            .bind(&digest)
            .bind(replay::REPLAY_WINDOW)
            .bind(draft.cancelled_through_sequence)
            .bind(Status::Settled.as_str())
            .bind(request.save)
            .fetch_one(connection)
            .await
            .map_err(query(CONTEXT))?;
        admitted.accepted(&person.id, &digest, sequence, identifiers)
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
    pub(crate) save: bool,
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
        .bind(insertion.save)
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
            digest: &message_digest(insertion.text, insertion.save)?,
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

fn message_digest(text: &str, save: bool) -> Result<String> {
    // Preserve existing retry receipts when the new flag is absent or false.
    if save {
        replay::digest(&(text, save))
    } else {
        replay::digest(&text)
    }
}
