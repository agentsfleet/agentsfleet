//! Domain operations share handles and confirmed targets, never a global request lock.

use std::borrow::Cow;

use af_core::timing::STREAM_ANSWER_TIMEOUT;
use af_wire::{protocol::Answer, runner::{HeartbeatResponse, HeartbeatStatus}};
use afd_runner::Beat;
use serde::Serialize;
use tokio::time::Instant;

use crate::{Operation, Result, error::within};
use super::{Message, SessionServices};

impl SessionServices {
    pub(super) async fn send(&self, message: &Message<'_>) -> Result<()> {
        let deadline = Instant::now() + STREAM_ANSWER_TIMEOUT;
        let kind = message.stream_kind();
        let frame = af_stream::encode(message, &self.gateway.0.output_budget, kind,
            &self.handle.stop, deadline).await?;
        self.output.send(kind, frame, &self.handle.stop, deadline).await
    }

    pub(super) async fn operation<T>(
        &self, operation: Operation, future: impl Future<Output = Result<T>>,
    ) -> Result<T> {
        let began = Instant::now();
        let result = within(&self.handle.stop, began + STREAM_ANSWER_TIMEOUT, operation, future).await;
        if let Err(error) = &result { self.gateway.observe(error, operation); }
        result
    }

    pub(super) fn policy<'a>(&self, beat: &'a Beat) -> Result<HeartbeatResponse<'a>> {
        Ok(HeartbeatResponse {
            status: HeartbeatStatus::Ok, assigned_policy: beat.assignment.decode()?,
            degraded: beat.verdict.is_degraded(), degraded_reason: beat.verdict.reason().map(Cow::Borrowed),
            selftest_requested: beat.selftest_requested,
            release_holds: beat.release_holds.iter().map(|id| id.as_str().into()).collect(),
            heartbeat_interval_ms: super::opening::interval_millis()?, woken: None,
        })
    }

    pub(super) async fn cancel(&self, lease: &af_core::id::Uuid7, code: &'static str) -> Result<()> {
        self.cache.cancel(lease);
        self.send(&Message::Cancel { lease_id: lease.as_str().into(), error_code: code.into() }).await
    }

    pub(super) fn apply_policy(&self, beat: &Beat) {
        self.handle.snapshot.send_modify(|state| state.degraded = beat.verdict.is_degraded());
    }

    pub(super) async fn recheck(&self) -> Result<()> {
        let beat = self.operation(Operation::Recheck, async {
            super::opening::allowed(self.gateway.0.dependencies.runners.recheck_connection(&self.authority).await?)
        }).await?;
        self.apply_policy(&beat);
        self.send(&Message::Assignment { response: self.policy(&beat)? }).await?;
        self.refresh_cancellations().await
    }

    pub(super) async fn refresh_cancellations(&self) -> Result<()> {
        let cancelled = self.operation(Operation::Recheck,
            self.cache.refresh(self.leases.leases.pool(), &self.handle.runner)).await?;
        for lease in cancelled {
            self.cancel(&lease, af_core::error_code::RUN_LEASE_CANCELLED.as_str()).await?;
        }
        Ok(())
    }
}

pub(super) fn answer<T: Serialize>(result: Result<T>) -> Answer<'static, T> {
    match result {
        Ok(response) => Answer::Ok { response },
        Err(error) => Answer::Refused { error_code: error.code().as_str().into(),
            detail: error.detail().into_owned().into(), retryable: error.retryable() },
    }
}

