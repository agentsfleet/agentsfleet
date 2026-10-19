//! Exact accounting replay never reads, recreates or returns private answer text.

use afd_wire::authoring::Reported;
use jiff::Timestamp;
use sqlx::PgConnection;

use super::{CONTEXT, prepare::Prepared, sql};
use crate::{
    Drafts, Result, draft,
    error::{
        ErrorKind,
        raise::{query, stored},
    },
    lease::guard::Attempt,
};

pub(super) async fn find(
    connection: &mut PgConnection,
    attempt: &Attempt,
    request: &Prepared,
) -> Result<Option<Reported>> {
    let prior: Option<(String, String)> = sqlx::query_as(sql::RECEIPT)
        .bind(attempt.id.as_str())
        .bind(request.report_id.as_str())
        .fetch_optional(&mut *connection)
        .await
        .map_err(query(CONTEXT))?;
    let Some((digest, acknowledgement)) = prior else {
        return Ok(None);
    };
    if digest != request.digest {
        return Err(ErrorKind::Replay.into());
    }
    if let Some(digest) = &request.answer_digest {
        let saved: Option<String> = sqlx::query_scalar(sql::ANSWER)
            .bind(attempt.id.as_str())
            .bind(request.report_id.as_str())
            .fetch_optional(connection)
            .await
            .map_err(query(CONTEXT))?;
        if saved.as_ref().is_some_and(|saved| saved != digest) {
            return Err(ErrorKind::Replay.into());
        }
    }
    let mut ack: Reported = draft::value(&acknowledgement)?;
    ack.replayed = true;
    Ok(Some(ack))
}

pub(super) async fn insert(
    drafts: &Drafts,
    connection: &mut PgConnection,
    attempt: &Attempt,
    request: &Prepared,
    ack: &Reported,
    now: Timestamp,
) -> Result<()> {
    sqlx::query(sql::ACKNOWLEDGE)
        .bind(drafts.mint(now)?.as_str())
        .bind(attempt.id.as_str())
        .bind(request.report_id.as_str())
        .bind(&request.digest)
        .bind(serde_json::to_string(ack).map_err(stored)?)
        .execute(connection)
        .await
        .map_err(query(CONTEXT))?;
    Ok(())
}

/// Files the receipt of the private answer whose digest is `digest`.
pub(super) async fn answer(
    drafts: &Drafts,
    connection: &mut PgConnection,
    attempt: &Attempt,
    prepared: &Prepared,
    digest: &str,
    now: Timestamp,
) -> Result<()> {
    sqlx::query(sql::ANSWER_RECEIPT)
        .bind(drafts.mint(now)?.as_str())
        .bind(attempt.draft_id.as_str())
        .bind(attempt.id.as_str())
        .bind(prepared.report_id.as_str())
        .bind(digest)
        .bind(crate::model::budget::PRIVATE_TTL)
        .execute(connection)
        .await
        .map_err(query(CONTEXT))?;
    Ok(())
}
