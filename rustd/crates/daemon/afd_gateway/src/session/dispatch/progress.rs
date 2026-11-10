//! Cosmetic progress uses the live target cache; input claims keep their database fence.

use std::num::NonZeroU64;

use af_core::id::Uuid7;
use af_wire::{activity::ActivityRequest, fold::FoldRequest};
use garde::Validate as _;

use super::super::{Message, SessionServices, context::answer};
use crate::{Operation, Result};

impl SessionServices {
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
        let Some(target) = self
            .cache
            .target(&id, self.gateway.inner.dependencies.wall_clock.now())
        else {
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
