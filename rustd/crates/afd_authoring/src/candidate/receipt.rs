//! Content-free activity acknowledgement outlives changes to the candidate head.

use crate::{
    Result,
    error::{ErrorKind, raise::query},
    lease::guard::Guarded,
};
use afd_core::id::Uuid7;
use afd_wire::authoring::{ActivityAccepted, CandidateVersion};
use sqlx::{PgConnection, Row as _};

const CONTEXT: &str = "read or save authoring activity receipt";
const READ: &str = "\
SELECT request_digest,candidate_id::text,candidate_revision FROM fleet.authoring_activity_receipts
WHERE attempt_id=$1::uuid AND activity_sequence=$2";
const INSERT: &str = "\
INSERT INTO fleet.authoring_activity_receipts
(id,attempt_id,activity_sequence,request_digest,candidate_id,candidate_revision,created_at)
VALUES($1::uuid,$2::uuid,$3,$4,$5::uuid,$6,$7)";

pub(super) async fn find(
    connection: &mut PgConnection,
    attempt: &Uuid7,
    sequence: i64,
    digest: &str,
) -> Result<Option<ActivityAccepted>> {
    let row = sqlx::query(READ)
        .bind(attempt.as_str())
        .bind(sequence)
        .fetch_optional(connection)
        .await
        .map_err(query(CONTEXT))?;
    let Some(row) = row else { return Ok(None) };
    let original: &str = row.try_get(0).map_err(query(CONTEXT))?;
    if original != digest {
        return Err(ErrorKind::Replay.into());
    }
    let id: Option<String> = row.try_get(1).map_err(query(CONTEXT))?;
    let revision: Option<i64> = row.try_get(2).map_err(query(CONTEXT))?;
    let candidate = id.zip(revision).map(|(id, revision)| CandidateVersion {
        id,
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
    sqlx::query(INSERT)
        .bind(id.as_str())
        .bind(guarded.attempt.id.as_str())
        .bind(sequence)
        .bind(digest)
        .bind(candidate.map(|c| c.id.as_str()))
        .bind(candidate.map(|_c| guarded.draft.candidate_revision + 1))
        .bind(guarded.now.as_millis())
        .execute(connection)
        .await
        .map_err(query(CONTEXT))?;
    Ok(())
}
