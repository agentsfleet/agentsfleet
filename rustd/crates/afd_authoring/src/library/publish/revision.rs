//! A selected Library advances under its strong tag and keeps old revisions.

use super::{CONTEXT, Replay, persist};
use crate::{
    Access, Result, Setup, access,
    error::{ErrorKind, raise::query},
    funnel,
    library::{read, source, sql},
    replay,
    types::{Candidate, PublishRevision, Published},
};
use afd_core::id::Uuid7;
use afd_observability::AuthoringStep;
use jiff::Timestamp;
use sqlx::{Connection as _, PgConnection};

impl Setup {
    /// Explicitly appends a revision to an owned Library entry.
    ///
    /// # Errors
    /// Requires an idle current candidate and current Library tag, after replay.
    pub async fn publish_revision(
        &self,
        access: Access<'_>,
        library: &str,
        expected: Option<&str>,
        operation: &Uuid7,
        request: PublishRevision,
        now: Timestamp,
    ) -> Result<Published> {
        let request = garde::Unvalidated::new(request).validate()?;
        let mut connection = self.drafts.database.acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        let person = access::current(&mut tx, access).await?;
        let scope = format!("{}:library:{library}:revision", access.workspace);
        let digest = replay::digest(&*request)?;
        let key = Replay {
            person: &person,
            scope: &scope,
            operation,
            digest: &digest,
        };
        if let Some(found) = key.existing(&mut tx).await? {
            tx.commit().await.map_err(query(CONTEXT))?;
            return Ok(found);
        }
        let (draft, candidate) =
            source::capture(&mut tx, access, &person, &request.draft_candidate_id).await?;
        let entry = read::entry(&mut tx, &person.owner, library, true).await?;
        if let Some(found) = key.existing(&mut tx).await? {
            tx.commit().await.map_err(query(CONTEXT))?;
            return Ok(found);
        }
        let expected = expected.ok_or(ErrorKind::Precondition)?;
        if expected != entry.etag() {
            return Err(ErrorKind::Stale { etag: entry.etag() }.into());
        }
        source::publishable(&mut tx, &draft, &candidate).await?;
        let head = read::revision(&mut tx, &entry.id, &entry.head_revision_id).await?;
        let revision = self.drafts.mint(now)?;
        let receipt_id = self.drafts.mint(now)?;
        let reservation = key.reservation(&receipt_id, access.workspace, &revision);
        let fresh = replay::reserve(&mut tx, reservation).await?;
        let result = if fresh {
            let next = persist::NewRevision {
                library: &entry.id,
                id: &revision,
                number: head.revision + 1,
            };
            advance(&mut tx, next, &draft.id, &candidate).await?;
            super::accepted(&mut tx, &person.owner, &revision).await?
        } else {
            key.replayed(&mut tx).await?
        };
        tx.commit().await.map_err(query(CONTEXT))?;
        if fresh {
            let step = AuthoringStep::LibraryPublished {
                library_id: entry.id,
                revision_id: revision,
                kind: funnel::PUBLISHED_REVISION,
            };
            self.drafts.reached(access, step);
        }
        Ok(result)
    }
}

/// Appends `next` and moves the entry's head onto it, in the caller's
/// transaction: the revision row first, so the head never names a revision
/// that does not exist yet.
async fn advance(
    tx: &mut PgConnection,
    next: persist::NewRevision<'_>,
    draft: &Uuid7,
    candidate: &Candidate,
) -> Result<()> {
    let (library, revision) = (next.library, next.id);
    persist::append(tx, next, draft, candidate).await?;
    sqlx::query(sql::ADVANCE)
        .bind(library.as_str())
        .bind(revision.as_str())
        .execute(&mut *tx)
        .await
        .map_err(query(CONTEXT))?;
    Ok(())
}
