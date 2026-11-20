//! A setup run's live answer and its compaction's progress, republished on
//! the draft's private stream in the frames the chat clients already read
//! (`message.delta`, `context.progress`).
//!
//! The runner streams answer chunks and compaction progress on the run's
//! activity verb, as a fleet run does. Before a batch is published the daemon rechecks, under no lock,
//! that the run is still the runner's live one and its message still
//! running, and that the draft's creator still holds the workspace: private
//! text never reaches a reader who lost access. The frame is cosmetic; the
//! report's answer is the durable one.

use af_core::{error_code, id::Uuid7};
use af_wire::context::ContextProgress;
use garde::Validate as _;
use zeroize::Zeroizing;

use super::progress::{ContextNotice, DELTA_BYTES, Delta, TextKind};
use super::sql;
use crate::{
    Access, Drafts, Result, access,
    error::{ErrorKind, raise::query},
    types::RUNNING,
};

/// One answer chunk a setup run's runner streamed.
///
/// `stream_seq` counts from zero across the whole run and `stream_start`
/// marks only the first chunk: a run is one reply, and the run id is its
/// stream identity, so a reassigned turn starts a new reply.
#[derive(Debug, Clone, Copy)]
pub struct PreviewChunk<'a> {
    /// Zero-based position within the run's answer.
    pub stream_seq: u64,
    /// The run's first chunk.
    pub stream_start: bool,
    /// The answer text, at most one frame's bytes.
    pub text: &'a str,
}

/// [`sql::PREVIEW`]'s row, by column name.
#[derive(sqlx::FromRow)]
struct Publishing {
    draft_id: Uuid7,
    sequence: i64,
    workspace_id: Uuid7,
    oidc_subject: String,
    user_id: Uuid7,
}

impl Drafts {
    /// Publishes `chunks` of `run`'s live answer to its draft's private
    /// stream, while the run is `runner`'s live one and its creator still
    /// holds the draft. A chunk too large for one frame, or empty, is dropped;
    /// the reader sees the gap and reads the durable answer.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, and a bus that would not
    /// take a frame.
    pub async fn publish_setup_preview(
        &self,
        runner: &Uuid7,
        run: &Uuid7,
        chunks: &[PreviewChunk<'_>],
    ) -> Result<()> {
        let Some(bus) = &self.bus else {
            return Ok(());
        };
        if chunks.is_empty() {
            return Ok(());
        }
        let Some(publishing) = self.preview_authority(runner, run).await? else {
            return Ok(());
        };
        let subject = afd_bus::Subject::ThreadItems(publishing.draft_id.clone());
        for chunk in chunks {
            if chunk.text.is_empty() || chunk.text.len() > DELTA_BYTES {
                continue;
            }
            let frame = Delta {
                draft_id: publishing.draft_id.clone(),
                sequence: publishing.sequence.to_string(),
                stream_id: run.clone(),
                stream_seq: chunk.stream_seq.saturating_add(1),
                stream_start: chunk.stream_start,
                text_kind: TextKind::Answer,
                text: chunk.text.to_owned(),
            };
            let payload =
                Zeroizing::new(serde_json::to_string(&frame).map_err(crate::error::raise::stored)?);
            bus.publish(&subject, &payload)
                .await
                .map_err(|source| ErrorKind::Preview { source })?;
        }
        Ok(())
    }

    /// Publishes `progress`, `run`'s compaction status, to its draft's
    /// private stream as `context.progress`, under the same authority as its
    /// answer: while the run is `runner`'s live one and its creator still
    /// holds the draft. A status outside its bounds is dropped.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, and a bus that would not
    /// take a frame.
    pub async fn publish_setup_context(
        &self,
        runner: &Uuid7,
        run: &Uuid7,
        progress: &[ContextProgress],
    ) -> Result<()> {
        let Some(bus) = &self.bus else {
            return Ok(());
        };
        if progress.is_empty() {
            return Ok(());
        }
        let Some(publishing) = self.preview_authority(runner, run).await? else {
            return Ok(());
        };
        let subject = afd_bus::Subject::ThreadItems(publishing.draft_id.clone());
        for status in progress {
            let frame = ContextNotice {
                draft_id: publishing.draft_id.clone(),
                sequence: publishing.sequence.to_string(),
                stream_id: run.clone(),
                progress: *status,
            };
            if frame.validate().is_err() {
                continue;
            }
            let payload = serde_json::to_string(&frame).map_err(crate::error::raise::stored)?;
            bus.publish(&subject, &payload)
                .await
                .map_err(|source| ErrorKind::Preview { source })?;
        }
        Ok(())
    }

    /// The draft and message `run` may publish for, or `None` once it may not.
    async fn preview_authority(&self, runner: &Uuid7, run: &Uuid7) -> Result<Option<Publishing>> {
        let mut connection = self.database.acquire().await?;
        let found: Option<Publishing> = sqlx::query_as(sql::PREVIEW)
            .bind(run)
            .bind(runner)
            .bind(afd_state::sql::RUN_STATUS_ACTIVE)
            .bind(RUNNING)
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(sql::CONTEXT))?;
        let Some(publishing) = found else {
            return Ok(None);
        };
        let access = Access {
            workspace: &publishing.workspace_id,
            subject: &publishing.oidc_subject,
        };
        match access::current(&mut connection, access).await {
            Ok(person) if person.id == publishing.user_id => Ok(Some(publishing)),
            Ok(_) => Ok(None),
            // A creator who lost the workspace is hidden, as every private read hides them.
            Err(error) if error.code() == error_code::AUTHORING_NOT_FOUND => Ok(None),
            Err(error) => Err(error),
        }
    }
}
