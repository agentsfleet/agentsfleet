//! Independent lease outcomes share one absolute batch deadline and bounded database concurrency.

use std::num::NonZeroU64;
use af_core::{id::Uuid7, timing::STREAM_ANSWER_TIMEOUT};
use af_wire::{protocol::{Renewal, RenewalAnswer}, report::RenewResponse};
use futures_util::{StreamExt as _, stream};
use garde::Validate as _;
use tokio::time::Instant;
use crate::{Operation, Result, error::within};
use super::super::{Message, SessionServices, context::answer};

impl SessionServices {
    /// A slow lease cannot turn a sixty-four-entry batch into sixty-four sequential timeouts.
    pub(in super::super) async fn renew(&self, request_id: NonZeroU64, leases: Vec<Renewal<'_>>) -> Result<()> {
        let deadline = Instant::now() + STREAM_ANSWER_TIMEOUT;
        let replies = stream::iter(leases).map(|renewal| self.renew_one(renewal, deadline))
            .buffer_unordered(8).collect().await;
        self.send(&Message::Renewed { request_id, leases: replies }).await
    }

    async fn renew_one<'a>(&self, renewal: Renewal<'a>, deadline: Instant) -> RenewalAnswer<'a> {
        let Renewal { lease_id, request } = renewal;
        let result = self.operation(Operation::Renewal, within(&self.handle.stop, deadline, Operation::Renewal, async {
            let id = Uuid7::parse(&lease_id)?;
            request.validate()?;
            let recorded_through = request.requests.0.iter().map(|line| line.seq).max();
            let (lease_expires_at, _charged) = self.leases.renew(&self.handle.runner, &id,
                request, self.gateway.0.dependencies.wall_clock.now()).await?;
            self.cache.renew(&id, lease_expires_at);
            Ok(RenewResponse { lease_expires_at, recorded_through })
        })).await;
        if result.as_ref().is_err_and(|error| !error.retryable()) {
            if let Ok(id) = Uuid7::parse(&lease_id) { self.cache.forget(&id); }
        }
        RenewalAnswer { lease_id, answer: answer(result) }
    }
}
