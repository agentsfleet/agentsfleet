//! Revalidate funded inputs without rebuilding or remeasuring the provider body.
use super::{ModelCallInput, continuation, source::Source};
use crate::{Result, error::ErrorKind, lease::guard, replay};
use afd_context::repository::{Repository, Scope};
use afd_context::{Checkpoint, CheckpointStore as _, Group, HistorySource as _};
use sqlx::PgConnection;

#[derive(Default, PartialEq, Eq)]
pub(super) struct Fingerprint {
    through: u64,
    digest: String,
    has_more: bool,
}

impl Fingerprint {
    pub(super) fn include(&mut self, groups: &[Group], has_more: bool) {
        self.digest = afd_context::digest_groups(&self.digest, groups);
        self.through = groups.last().map_or(self.through, |group| group.sequence);
        self.has_more = has_more;
    }
}

pub(in crate::model) struct Snapshot {
    identity: String,
    checkpoint: String,
    continuations: String,
    pub(super) source: Fingerprint,
}

impl Snapshot {
    pub(super) fn new(
        attempt: &guard::Attempt,
        editing: bool,
        checkpoint: Option<&Checkpoint>,
        continuations: &[continuation::Continuation],
    ) -> Result<Self> {
        Ok(Self {
            identity: identity(attempt, editing)?,
            checkpoint: replay::digest(&checkpoint)?,
            continuations: replay::digest(&continuations)?,
            source: Fingerprint::default(),
        })
    }

    pub(in crate::model) async fn verify(
        self,
        connection: &mut PgConnection,
        guarded: &guard::Guarded,
        request: &garde::Valid<ModelCallInput>,
    ) -> Result<()> {
        let attempt = &guarded.attempt;
        let checkpoint = Repository::new(
            connection,
            Scope::Draft(attempt.draft_id.clone()),
            attempt.id.clone(),
        )
        .load()
        .await?;
        let continuations = continuation::read(connection, attempt, request, guarded.now).await?;
        if self.identity != identity(attempt, guarded.draft.head_candidate_id.is_some())?
            || self.checkpoint != replay::digest(&checkpoint)?
            || self.continuations != replay::digest(&continuations)?
        {
            return Err(ErrorKind::Fence.into());
        }
        let mut source = Source::new(connection, attempt);
        loop {
            let page = source
                .page(source.fingerprint.through, afd_context::HISTORY_PAGE_SIZE)
                .await?;
            if !page.has_more
                || (self.source.has_more && source.fingerprint.through >= self.source.through)
            {
                break;
            }
        }
        if source.fingerprint != self.source {
            return Err(ErrorKind::Fence.into());
        }
        Ok(())
    }
}

fn identity(attempt: &guard::Attempt, editing: bool) -> Result<String> {
    replay::digest(&(
        attempt.id.as_str(),
        attempt.draft_id.as_str(),
        attempt.message_sequence,
        &attempt.model_snapshot,
        editing,
    ))
}
