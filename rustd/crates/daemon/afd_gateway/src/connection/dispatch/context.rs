//! A run's compacted context, a setup draft's or a fleet chat's, read and
//! saved on the run's own connection: the read is `afd_fleet`'s, the fenced
//! save `afd_setup`'s.
//!
//! The checkpoint crosses the wire as the opaque document `af_wire` carries,
//! so it is decoded here, into the `af_context` type both halves check.

use std::num::NonZeroU64;

use af_context::Checkpoint;
use af_core::id::Uuid7;
use af_wire::context::{
    ContextLoadRequest, ContextLoadResponse, ContextSaveRequest, ContextSaveResponse,
};
use afd_setup::setup_run::ContextSave;
use garde::Validate as _;

use super::super::{ConnectionServices, Message, context::answer};
use crate::{Operation, Result};

impl ConnectionServices {
    pub(in super::super) async fn context_load(
        &self,
        request_id: NonZeroU64,
        run: &str,
        request: &ContextLoadRequest,
    ) -> Result<()> {
        let result = self
            .operation(Operation::Context, async {
                let id = Uuid7::parse(run)?;
                request.validate()?;
                let now = self.gateway.inner.dependencies.wall_clock.now();
                let loaded = self
                    .runs
                    .load_context(&self.handle.runner, &id, request, now)
                    .await?;
                Ok(ContextLoadResponse {
                    scope: loaded.scope,
                    resource_version: loaded
                        .checkpoint
                        .as_ref()
                        .map_or(0, |checkpoint| checkpoint.generation),
                    checkpoint: loaded
                        .checkpoint
                        .as_ref()
                        .map(serde_json::to_value)
                        .transpose()?,
                })
            })
            .await;
        self.send(&Message::ContextLoaded {
            request_id,
            run_id: run.into(),
            answer: answer(result),
        })
        .await
    }

    pub(in super::super) async fn context_save(
        &self,
        request_id: NonZeroU64,
        run: &str,
        request: &ContextSaveRequest,
    ) -> Result<()> {
        let result = self
            .operation(Operation::Context, async {
                let id = Uuid7::parse(run)?;
                request.validate()?;
                let checkpoint: Checkpoint = serde_json::from_value(request.checkpoint.clone())?;
                let generation = checkpoint.generation;
                let now = self.gateway.inner.dependencies.wall_clock.now();
                let save = ContextSave {
                    run_id: id,
                    fence: request.fencing_token,
                    resource_version: request.resource_version,
                    checkpoint,
                };
                self.drafts
                    .save_context(&self.handle.runner, save, now)
                    .await?;
                Ok(ContextSaveResponse {
                    resource_version: generation,
                })
            })
            .await;
        self.send(&Message::ContextSaved {
            request_id,
            run_id: run.into(),
            answer: answer(result),
        })
        .await
    }
}
