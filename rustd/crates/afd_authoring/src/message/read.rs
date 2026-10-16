//! Message cursors bind creator, workspace, draft, ordering and page size.

use crate::{
    Access, Drafts, Result, access,
    draft::{self, read},
    error::{ErrorKind, raise::query},
    replay, sql,
    types::{Message, MessageQuery, MessageState, Page, QUEUED, RUNNING},
};
use afd_core::{id::Uuid7, paging::Cursor};
use serde::Deserialize;
use sqlx::{Connection as _, Row as _};

const CONTEXT: &str = "read private message page";
const USER: &str = "user";
const ASSISTANT: &str = "assistant";
const CURSOR_RESOURCE: &str = "draft-messages-sequence-role";

#[derive(Deserialize)]
struct Stored {
    sequence: i64,
    operation_id: Uuid7,
    state: MessageState,
    user_text: String,
    answer_text: Option<String>,
    candidate_id: Option<Uuid7>,
    usage: Option<afd_wire::authoring::Usage>,
    failure_reason: Option<afd_wire::authoring::FailureReason>,
    created_at: i64,
    finished_at: Option<i64>,
}

impl Drafts {
    /// Reads a bounded page of the creator's durable conversation.
    ///
    /// # Errors
    /// Refuses foreign/malformed cursors and revoked or inaccessible drafts.
    pub async fn messages(
        &self,
        access: Access<'_>,
        id: &Uuid7,
        request: MessageQuery,
    ) -> Result<Page<Message>> {
        let request = garde::Unvalidated::new(request).validate()?;
        let mut connection = self.database.acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        let person = access::current(&mut tx, access).await?;
        read::lock(&mut tx, access, &person, id).await?;
        let binding = replay::digest(&(
            CURSOR_RESOURCE,
            access.workspace,
            &person.id,
            id,
            request.limit,
        ))?;
        let (sequence, ordinal) = boundary(request.starting_after.as_deref(), &binding)?;
        let rows = sqlx::query(sql::message::PAGE)
            .bind(id.as_str())
            .bind(sequence)
            .bind(ordinal)
            .bind(QUEUED)
            .bind(RUNNING)
            .bind(i64::from(request.limit) + 1)
            .fetch_all(&mut *tx)
            .await
            .map_err(query(CONTEXT))?;
        let mut items = Vec::new();
        let more = rows.len() > request.limit as usize;
        let mut next_cursor = None;
        for row in rows {
            if items.len() == request.limit as usize {
                break;
            }
            let stored: Stored =
                draft::value(&row.try_get::<String, _>(0).map_err(query(CONTEXT))?)?;
            let role: i32 = row.try_get(1).map_err(query(CONTEXT))?;
            next_cursor = Some(cursor(&binding, stored.sequence, role));
            items.push(project(stored, role));
        }
        tx.commit().await.map_err(query(CONTEXT))?;
        Ok(Page {
            items,
            total: None,
            next_cursor: if more { next_cursor } else { None },
        })
    }
}

fn project(stored: Stored, role: i32) -> Message {
    Message {
        sequence: stored.sequence.to_string(),
        operation_id: stored.operation_id,
        role: if role == 0 { USER } else { ASSISTANT },
        state: stored.state,
        text: if role == 0 {
            Some(stored.user_text)
        } else {
            stored.answer_text
        },
        candidate_id: if role == 0 { None } else { stored.candidate_id },
        usage: stored.usage,
        failure: stored.failure_reason.map(Into::into),
        created_at: stored.created_at,
        finished_at: stored.finished_at,
    }
}

fn boundary(raw: Option<&str>, binding: &str) -> Result<(i64, i32)> {
    let Some(raw) = raw else {
        return Ok((0, -1));
    };
    let Cursor::Text { value, id } = Cursor::parse(raw).map_err(|_invalid| ErrorKind::Cursor)?
    else {
        return Err(ErrorKind::Cursor.into());
    };
    if value != binding {
        return Err(ErrorKind::Cursor.into());
    }
    let (sequence, ordinal) = id.split_once(':').ok_or(ErrorKind::Cursor)?;
    let sequence = sequence
        .parse::<i64>()
        .map_err(|_invalid| ErrorKind::Cursor)?;
    let ordinal = ordinal
        .parse::<i32>()
        .map_err(|_invalid| ErrorKind::Cursor)?;
    if sequence <= 0 || !(0..=1).contains(&ordinal) {
        return Err(ErrorKind::Cursor.into());
    }
    Ok((sequence, ordinal))
}

fn cursor(binding: &str, sequence: i64, ordinal: i32) -> String {
    Cursor::Text {
        value: binding.to_owned(),
        id: format!("{sequence}:{ordinal}"),
    }
    .to_string()
}
