//! Replay precedes idle/tag checks so a lost successful answer remains recoverable.

use super::super::{CONTEXT, Creations, KILLED, etag, read, source::Captured};
use crate::{
    Access, Result, access, draft,
    error::{ErrorKind, raise::query},
    replay, review,
    types::{AppliedRevision, ApplyRevision, SaveSource},
};
use afd_core::id::Uuid7;
use jiff::Timestamp;
use sqlx::Connection as _;

impl Creations {
    /// Applies one reviewed improvement without publishing a Library revision.
    ///
    /// # Errors
    /// Refuses stale setup tags, changed bases, foreign candidates, active turns,
    /// policy drift and changed replay input before any setup write commits.
    pub async fn apply_revision(
        &self,
        access: Access<'_>,
        fleet: &Uuid7,
        expected: Option<&str>,
        operation: &Uuid7,
        request: ApplyRevision,
        now: Timestamp,
    ) -> Result<AppliedRevision> {
        let request = garde::Unvalidated::new(request).validate()?;
        let revision = self
            .replace(access, fleet, expected, operation, &request, now)
            .await?;
        self.advance(access, fleet, now).await?;
        self.applied(access, fleet, &revision, &request.review_digest)
            .await
    }

    async fn replace(
        &self,
        access: Access<'_>,
        fleet: &Uuid7,
        expected: Option<&str>,
        operation: &Uuid7,
        request: &garde::Valid<ApplyRevision>,
        now: Timestamp,
    ) -> Result<Uuid7> {
        let mut connection = self.setup.drafts.database.acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        let person = access::current(&mut tx, access).await?;
        let scope = format!("{}:fleet:{fleet}:revision", access.workspace);
        let digest = replay::digest(&**request)?;
        if let Some(id) = replay::find(
            &mut tx,
            &scope,
            replay::Resource::FleetRevision,
            operation,
            &person.id,
            &digest,
        )
        .await?
        {
            return Ok(id);
        }
        let captured = self
            .capture(
                &mut tx,
                access,
                &person,
                &SaveSource::Draft {
                    draft_candidate_id: request.draft_candidate_id.clone(),
                },
            )
            .await?;
        let row = read::lock(&mut tx, access.workspace, fleet).await?;
        if let Some(id) = replay::find(
            &mut tx,
            &scope,
            replay::Resource::FleetRevision,
            operation,
            &person.id,
            &digest,
        )
        .await?
        {
            return Ok(id);
        }
        validate(&captured, &row, expected)?;
        draft::idle(&mut tx, captured.draft.as_ref().ok_or(ErrorKind::Hidden)?).await?;
        let checked = review::build(
            &self.setup,
            &mut tx,
            &captured.context,
            &captured.candidate,
            now,
        )
        .await?;
        checked.require_digest(&request.review_digest)?;
        let revision = self.setup.drafts.mint(now)?;
        let won = replay::reserve(
            &mut tx,
            replay::Reservation {
                receipt_id: &self.setup.drafts.mint(now)?,
                workspace: access.workspace,
                person: &person,
                scope: &scope,
                operation,
                digest: &digest,
                resource: &revision,
            },
        )
        .await?;
        let mut model_changed = false;
        let revision = if won {
            model_changed = self
                .write_improvement(&mut tx, &row, &captured, &checked, &revision, now)
                .await?;
            revision
        } else {
            replay::find(
                &mut tx,
                &scope,
                replay::Resource::FleetRevision,
                operation,
                &person.id,
                &digest,
            )
            .await?
            .ok_or(ErrorKind::Gone)?
        };
        tx.commit().await.map_err(query(CONTEXT))?;
        if model_changed {
            super::activity::publish(&self.admissions.streams(), fleet).await;
        }
        Ok(revision)
    }
}

fn validate(
    captured: &Captured,
    row: &super::super::stored::Stored,
    expected: Option<&str>,
) -> Result<()> {
    let current = etag(&row.id, row.head_revision);
    if expected.ok_or(ErrorKind::Precondition)? != current {
        return Err(ErrorKind::Stale { etag: current }.into());
    }
    if row.status == KILLED {
        return Err(ErrorKind::State { current: "killed" }.into());
    }
    super::target(captured, &row.id, row.head_revision)?;
    captured.candidate.definition.validate_for_save()?;
    if captured.candidate.policy_semantics != afd_fleet_runtime::definition::POLICY_SEMANTICS {
        return Err(ErrorKind::Policy.into());
    }
    Ok(())
}
