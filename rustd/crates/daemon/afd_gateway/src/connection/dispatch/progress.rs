//! Cosmetic progress uses the live target cache; input claims keep their database fence.

use std::num::NonZeroU64;

use af_core::id::Uuid7;
use af_wire::{
    activity::{ActivityFrame, ActivityRequest, StreamTextKind},
    fold::FoldRequest,
};
use afd_setup::setup_lease::PreviewChunk;
use garde::Validate as _;

use super::super::{ConnectionServices, Message, context::answer};
use crate::{Operation, Result};

impl ConnectionServices {
    pub(in super::super) async fn progress(
        &self,
        lease: &str,
        request: &ActivityRequest<'_>,
    ) -> Result<()> {
        let id = Uuid7::parse(lease)?;
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
            .operation(Operation::Activity, async {
                Ok(self
                    .leases
                    .activity_for_target(&self.handle.runner, &id, &target, &request.frames)
                    .await?)
            })
            .await
        {
            crate::Gateway::observe(&error, Operation::Activity);
        }
        Ok(())
    }

    /// A draft lease's answer chunks, republished on its draft's private
    /// stream. Cosmetic: a refused publish is observed, never returned.
    async fn setup_preview(&self, lease: &Uuid7, frames: &[ActivityFrame<'_>]) -> Result<()> {
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
            .operation(Operation::Activity, async {
                Ok(self
                    .drafts
                    .publish_setup_preview(&self.handle.runner, lease, &chunks)
                    .await?)
            })
            .await
        {
            crate::Gateway::observe(&error, Operation::Activity);
        }
        Ok(())
    }

    pub(in super::super) async fn folds(
        &self,
        request_id: NonZeroU64,
        lease: &str,
        request: &FoldRequest,
    ) -> Result<()> {
        let result = self
            .operation(Operation::Folds, async {
                let id = Uuid7::parse(lease)?;
                request.validate()?;
                Ok(self.leases.folds(&self.handle.runner, &id, request).await?)
            })
            .await;
        self.send(&Message::Folded {
            request_id,
            lease_id: lease.into(),
            answer: answer(result),
        })
        .await
    }
}
