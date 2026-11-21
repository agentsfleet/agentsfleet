//! Cosmetic progress uses the live target cache; input assignments keep their database fence.

use std::num::NonZeroU64;

use af_core::id::Uuid7;
use af_wire::{
    activity::{ActivityFrame, ActivityRequest, StreamTextKind},
    context::ContextProgress,
    fold::{FoldRequest, FoldResponse},
    protocol::{Answer, MESSAGE_BYTES_MAX},
};
use afd_setup::setup_run::PreviewChunk;
use garde::Validate as _;

use super::super::{ConnectionServices, Message, context::answer};
use crate::{Operation, Result};

impl ConnectionServices {
    pub(in super::super) async fn progress(
        &self,
        run: &str,
        request: &ActivityRequest<'_>,
    ) -> Result<()> {
        let id = Uuid7::parse(run)?;
        for frame in &request.frames {
            frame.validate()?;
            if !frame.outcome_usable() {
                return Err(crate::error::ErrorKind::Protocol.into());
            }
        }
        let now = self.gateway.inner.dependencies.wall_clock.now();
        if self.cache.live_draft(&id, now) {
            return self.setup_preview(&id, &request.frames).await;
        }
        let Some(target) = self.cache.target(&id, now) else {
            return Ok(());
        };
        // A slow cosmetic publisher never holds control, renewal or report workers.
        if let Err(error) = self
            .operation(Operation::Progress, async {
                Ok(self
                    .runs
                    .activity_for_target(&self.handle.runner, &id, &target, &request.frames)
                    .await?)
            })
            .await
        {
            crate::Gateway::observe(&error, Operation::Progress);
        }
        Ok(())
    }

    /// A draft run's answer chunks and compaction progress, republished on
    /// its draft's private stream as `message.delta` and `context.progress`.
    /// Cosmetic: a refused publish is observed, never returned.
    async fn setup_preview(&self, run: &Uuid7, frames: &[ActivityFrame<'_>]) -> Result<()> {
        let compaction: Vec<ContextProgress> = frames
            .iter()
            .filter_map(|frame| match frame {
                ActivityFrame::ContextProgress(progress) => Some(*progress),
                _ => None,
            })
            .collect();
        let chunks: Vec<PreviewChunk<'_>> = frames
            .iter()
            .filter_map(|frame| match frame {
                ActivityFrame::FleetResponseChunk(chunk)
                    if chunk.text_kind == StreamTextKind::Answer =>
                {
                    Some(PreviewChunk {
                        stream_seq: chunk.stream_seq,
                        stream_start: chunk.stream_start,
                        text: chunk.text.as_ref(),
                    })
                }
                _ => None,
            })
            .collect();
        if let Err(error) = self
            .operation(Operation::Progress, async {
                self.drafts
                    .publish_setup_context(&self.handle.runner, run, &compaction)
                    .await?;
                Ok(self
                    .drafts
                    .publish_setup_preview(&self.handle.runner, run, &chunks)
                    .await?)
            })
            .await
        {
            crate::Gateway::observe(&error, Operation::Progress);
        }
        Ok(())
    }

    pub(in super::super) async fn folds(
        &self,
        request_id: NonZeroU64,
        run: &str,
        request: &FoldRequest,
    ) -> Result<()> {
        let result = self
            .operation(Operation::Folds, async {
                let id = Uuid7::parse(run)?;
                request.validate()?;
                let mut response = self.runs.folds(&self.handle.runner, &id, request).await?;
                within_frame(&mut response, request_id, run)?;
                Ok(response)
            })
            .await;
        self.send(&Message::Folded {
            request_id,
            run_id: run.into(),
            answer: answer(result),
        })
        .await
    }
}

/// Cuts a fold's reply to what one frame carries beside its envelope.
///
/// A fold takes every message the run has not read, each up to the
/// text ceiling, so together they can pass the frame, which the encoder would
/// refuse on every retry. The messages left out stay assigned to the run, and
/// its next fold, after the last one it read, returns them.
fn within_frame(response: &mut FoldResponse<'_>, request_id: NonZeroU64, run: &str) -> Result<()> {
    let empty = Message::Folded {
        request_id,
        run_id: run.into(),
        answer: Answer::Ok {
            response: FoldResponse::default(),
        },
    };
    let envelope = af_validate::serialized_len(&empty)?
        .saturating_sub(af_validate::serialized_len(&FoldResponse::default())?);
    response.keep_within(MESSAGE_BYTES_MAX.saturating_sub(envelope))?;
    Ok(())
}

#[cfg(test)]
#[path = "progress_tests.rs"]
mod tests;
