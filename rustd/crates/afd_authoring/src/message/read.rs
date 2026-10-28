//! Message cursors bind creator, workspace, draft, ordering and page size.

use crate::{
    Access, Drafts, Result, access,
    draft::{read, word},
    error::{ErrorKind, raise::query},
    lease::meter::Metered,
    model::dispatch_status,
    replay, sql,
    types::{Message, MessageQuery, MessageState, Page, QUEUED, RUNNING},
};
use afd_core::{id::Uuid7, paging::Cursor};
use jiff::Timestamp;
use sqlx::Connection as _;

const CONTEXT: &str = "read private message page";
const USER: &str = "user";
const ASSISTANT: &str = "assistant";
const CURSOR_RESOURCE: &str = "draft-messages-sequence-role";

/// [`sql::message::PAGE`]'s row, by column name.
#[derive(sqlx::FromRow)]
struct Stored {
    save_requested: bool,
    save_error_code: Option<String>,
    sequence: i64,
    operation_id: String,
    status: String,
    user_text: String,
    answer_text: Option<String>,
    candidate_id: Option<String>,
    failure_reason: Option<String>,
    failure_cause: Option<String>,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    created_at: Timestamp,
    finished_at: Option<jiff_sqlx::Timestamp>,
    ordinal: i32,
    #[sqlx(flatten)]
    metered: Metered,
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
        let draft = read::lock(&mut tx, access, &person, id).await?;
        let page = page_on(&mut tx, access, &person.id, &draft, &request).await?;
        tx.commit().await.map_err(query(CONTEXT))?;
        Ok(page)
    }
}

pub(super) async fn page_on(
    connection: &mut sqlx::PgConnection,
    access: Access<'_>,
    person: &Uuid7,
    draft: &read::Locked,
    request: &MessageQuery,
) -> Result<Page<Message>> {
    let binding = replay::digest(&(
        CURSOR_RESOURCE,
        access.workspace,
        person,
        &draft.id,
        request.limit,
    ))?;
    let (sequence, ordinal) = boundary(request.starting_after.as_deref(), &binding)?;
    let rows: Vec<Stored> = sqlx::query_as(sql::message::PAGE)
        .bind(draft.id.as_str())
        .bind(sequence)
        .bind(ordinal)
        .bind(QUEUED)
        .bind(RUNNING)
        .bind(i64::from(request.limit) + 1)
        .bind(dispatch_status::KNOWN)
        .bind(dispatch_status::PENDING)
        .bind(dispatch_status::UNKNOWN)
        .fetch_all(&mut *connection)
        .await
        .map_err(query(CONTEXT))?;
    let mut items = Vec::new();
    let more = rows.len() > request.limit as usize;
    let mut next_cursor = None;
    for row in rows {
        if items.len() == request.limit as usize {
            break;
        }
        next_cursor = Some(cursor(&binding, row.sequence, row.ordinal));
        items.push(project(row, draft.cancelled_through_sequence)?);
    }
    Ok(Page {
        items,
        total: None,
        next_cursor: if more { next_cursor } else { None },
    })
}

pub(super) fn recent_cursor(
    access: Access<'_>,
    person: &Uuid7,
    draft: &read::Locked,
) -> Result<Option<String>> {
    let before = draft.last_message_sequence.saturating_sub(12);
    let binding = replay::digest(&(CURSOR_RESOURCE, access.workspace, person, &draft.id, 25_u32))?;
    Ok((before > 0).then(|| cursor(&binding, before, 1)))
}

pub(super) fn window_cursors(
    access: Access<'_>,
    person: &Uuid7,
    draft: &read::Locked,
    messages: &[Message],
) -> Result<Vec<String>> {
    let binding = replay::digest(&(CURSOR_RESOURCE, access.workspace, person, &draft.id, 25_u32))?;
    Ok(messages
        .iter()
        .map(|message| {
            let ordinal = i32::from(message.role == ASSISTANT);
            Cursor::Text {
                value: binding.clone(),
                id: format!("{}:{ordinal}", message.sequence),
            }
            .to_string()
        })
        .collect())
}

fn project(stored: Stored, cancelled_through: i64) -> Result<Message> {
    let cause = stored
        .failure_cause
        .as_deref()
        .map(word::<crate::types::FailureCause>)
        .transpose()?;
    let user = stored.ordinal == 0;
    let state: MessageState = word(&stored.status)?;
    Ok(Message {
        save_requested: stored.save_requested,
        save_error_code: if user { None } else { stored.save_error_code },
        sequence: stored.sequence.to_string(),
        operation_id: Uuid7::parse(&stored.operation_id)?,
        role: if user { USER } else { ASSISTANT },
        state,
        text: if user {
            Some(stored.user_text)
        } else {
            stored.answer_text
        },
        candidate_id: if user {
            None
        } else {
            crate::draft::read::optional(stored.candidate_id.as_deref())?
        },
        usage: stored.metered.usage()?,
        failure: stored
            .failure_reason
            .as_deref()
            .map(word::<afd_wire::authoring::FailureReason>)
            .transpose()?
            .map(|reason| {
                crate::types::Failure::for_message(
                    reason,
                    stored.sequence,
                    cancelled_through,
                    cause,
                )
            }),
        created_at: stored.created_at,
        finished_at: stored.finished_at.map(jiff_sqlx::Timestamp::to_jiff),
    })
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
