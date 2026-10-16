//! A new publication does not close a draft or allocate a fleet.

use super::{CONTEXT, Replay, persist};
use crate::{
    Access, Result, Setup, access,
    error::raise::query,
    funnel,
    library::source,
    replay,
    types::{PublishLibrary, Published},
};
use afd_core::{clock::UnixMillis, id::Uuid7};
use afd_observability::AuthoringStep;
use sqlx::Connection as _;

impl Setup {
    /// Explicitly publishes an idle private candidate as a new Library entry.
    ///
    /// # Errors
    /// Refuses inaccessible/busy/invalid candidates and changed replay bodies.
    pub async fn publish(
        &self,
        access: Access<'_>,
        operation: &Uuid7,
        request: PublishLibrary,
        now: UnixMillis,
    ) -> Result<Published> {
        let request = garde::Unvalidated::new(request).validate()?;
        let mut connection = self.drafts.database.acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        let person = access::current(&mut tx, access).await?;
        let scope = format!("{}:library:create:{}", access.workspace, person.id);
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
        if let Some(found) = key.existing(&mut tx).await? {
            tx.commit().await.map_err(query(CONTEXT))?;
            return Ok(found);
        }
        source::publishable(&mut tx, &draft, &candidate).await?;
        let library = self.drafts.mint(now)?;
        let revision = self.drafts.mint(now)?;
        let receipt_id = self.drafts.mint(now)?;
        let reservation = key.reservation(&receipt_id, access.workspace, &revision, now);
        let fresh = replay::reserve(&mut tx, reservation).await?;
        let result = if fresh {
            persist::entry(
                &mut tx,
                &person.owner,
                &library,
                &revision,
                &request.label,
                &candidate,
                now,
            )
            .await?;
            let first = persist::NewRevision {
                library: &library,
                public_id: library.as_str(),
                id: &revision,
                number: 1,
            };
            persist::append(&mut tx, first, &draft.id, &candidate, now).await?;
            super::accepted(&mut tx, &person.owner, &revision).await?
        } else {
            key.replayed(&mut tx).await?
        };
        tx.commit().await.map_err(query(CONTEXT))?;
        if fresh {
            let step = AuthoringStep::LibraryPublished {
                library_id: library,
                revision_id: revision,
                kind: funnel::PUBLISHED_ENTRY,
            };
            self.drafts.reached(access, step);
        }
        Ok(result)
    }
}
