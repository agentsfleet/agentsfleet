//! Content-free activity acknowledgement outlives changes to the candidate head.

use super::sql;
use crate::{
    Result,
    error::{ErrorKind, raise::query},
    lease::guard::Guarded,
};
use af_core::id::Uuid7;
use af_wire::authoring::{ActivityAccepted, CandidateVersion};
use sqlx::PgConnection;

const CONTEXT: &str = "read or save authoring activity receipt";

/// [`sql::RECEIPT`]'s row, by column name.
#[derive(sqlx::FromRow)]
struct Receipt {
    request_digest: String,
    candidate_id: Option<Uuid7>,
    candidate_revision: Option<i64>,
}

pub(super) async fn find(
    connection: &mut PgConnection,
    attempt: &Uuid7,
    sequence: i64,
    digest: &str,
) -> Result<Option<ActivityAccepted>> {
    let row: Option<Receipt> = sqlx::query_as(sql::RECEIPT)
        .bind(attempt)
        .bind(sequence)
        .fetch_optional(connection)
        .await
        .map_err(query(CONTEXT))?;
    let Some(row) = row else { return Ok(None) };
    if row.request_digest != digest {
        return Err(ErrorKind::Replay.into());
    }
    let candidate = row
        .candidate_id
        .zip(row.candidate_revision)
        .map(|(id, revision)| CandidateVersion {
            id: id.into(),
            revision: revision.to_string(),
        });
    Ok(Some(ActivityAccepted {
        accepted: true,
        replayed: true,
        candidate,
    }))
}

pub(super) async fn insert(
    connection: &mut PgConnection,
    id: &Uuid7,
    guarded: &Guarded,
    sequence: i64,
    digest: &str,
    candidate: Option<&CandidateVersion>,
) -> Result<()> {
    // The wire carries the candidate's id as text; the column is a `uuid`.
    let candidate = candidate.map(|c| Uuid7::parse(&c.id)).transpose()?;
    sqlx::query(sql::INSERT_RECEIPT)
        .bind(id)
        .bind(&guarded.attempt.id)
        .bind(sequence)
        .bind(digest)
        .bind(candidate.as_ref())
        .execute(connection)
        .await
        .map_err(query(CONTEXT))?;
    Ok(())
}
