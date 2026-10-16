//! Acceptance pins sequence/model once and checks replay before current state.

use crate::{
    Access, Drafts, Result, access,
    draft::read,
    error::{
        ErrorKind,
        raise::{invalid, query},
    },
    replay, sql,
    types::{Accepted, DraftState, QUEUED, SendMessage},
};
use afd_core::{clock::UnixMillis, id::Uuid7};
use afd_observability::AuthoringStep;
use sqlx::{Connection as _, PgConnection, Row as _};

const CONTEXT: &str = "accept private draft message";
const SEQUENCE_EXHAUSTED: &str = "message sequence is exhausted";

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
        now: UnixMillis,
    ) -> Result<Accepted> {
        let request = garde::Unvalidated::new(request).validate()?;
        let text = super::normalize(&request.message);
        let digest = replay::digest(&text)?;
        let mut connection = self.database.acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        let person = access::current(&mut tx, access).await?;
        let draft = read::lock(&mut tx, access, &person, id).await?;
        let prior = sqlx::query(sql::message::REPLAY)
            .bind(id.as_str())
            .bind(request.operation_id.as_str())
            .fetch_optional(&mut *tx)
            .await
            .map_err(query(CONTEXT))?;
        let accepted = if let Some(row) = prior {
            let stored: String = row.try_get(1).map_err(query(CONTEXT))?;
            if stored != digest {
                return Err(ErrorKind::Replay.into());
            }
            Accepted {
                status: super::ACCEPTED,
                sequence: row
                    .try_get::<i64, _>(0)
                    .map_err(query(CONTEXT))?
                    .to_string(),
                replayed: true,
            }
        } else {
            if draft.state != DraftState::Editing {
                return Err(ErrorKind::State {
                    current: draft.state.as_str(),
                }
                .into());
            }
            let sequence = draft
                .last_sequence
                .checked_add(1)
                .ok_or_else(|| invalid(SEQUENCE_EXHAUSTED))?;
            insert(
                &mut tx,
                Insertion {
                    id: &self.mint(now)?,
                    draft: id,
                    sequence,
                    operation: &request.operation_id,
                    text: &text,
                    model: &draft.authoring_model_ref,
                    now,
                },
            )
            .await?;
            Accepted {
                status: super::ACCEPTED,
                sequence: sequence.to_string(),
                replayed: false,
            }
        };
        tx.commit().await.map_err(query(CONTEXT))?;
        if !accepted.replayed {
            let draft_id = id.clone();
            let step = AuthoringStep::MessageAccepted { draft_id };
            self.reached(access, step);
        }
        Ok(accepted)
    }
}

pub(crate) struct Insertion<'a> {
    pub(crate) id: &'a Uuid7,
    pub(crate) draft: &'a Uuid7,
    pub(crate) sequence: i64,
    pub(crate) operation: &'a Uuid7,
    pub(crate) text: &'a str,
    pub(crate) model: &'a str,
    pub(crate) now: UnixMillis,
}

pub(crate) async fn insert(connection: &mut PgConnection, insertion: Insertion<'_>) -> Result<()> {
    let Insertion {
        id,
        draft,
        sequence,
        operation,
        text,
        model,
        now,
    } = insertion;
    sqlx::query(sql::message::INSERT)
        .bind(id.as_str())
        .bind(draft.as_str())
        .bind(sequence)
        .bind(operation.as_str())
        .bind(replay::digest(&text)?)
        .bind(model)
        .bind(QUEUED)
        .bind(text)
        .bind(now.as_millis())
        .execute(&mut *connection)
        .await
        .map_err(query(CONTEXT))?;
    sqlx::query(sql::message::ADVANCE)
        .bind(draft.as_str())
        .bind(sequence)
        .bind(now.as_millis())
        .execute(connection)
        .await
        .map_err(query(CONTEXT))?;
    Ok(())
}
