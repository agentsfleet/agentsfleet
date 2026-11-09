//! Domain operations share handles and confirmed targets, never a global request lock.

use std::borrow::Cow;

use af_core::timing::STREAM_ANSWER_TIMEOUT;
use af_wire::{
    protocol::Answer,
    runner::{HeartbeatResponse, HeartbeatStatus},
};
use afd_runner::Beat;
use serde::Serialize;
use tokio::time::Instant;

use super::{Message, SessionServices};
use crate::{Operation, Result, error::within};

impl SessionServices {
    pub(super) async fn send(&self, message: &Message<'_>) -> Result<()> {
        let deadline = Instant::now() + STREAM_ANSWER_TIMEOUT;
        let kind = message.stream_kind();
        let frame = af_stream::encode(
            message,
            &self.gateway.inner.output_budget,
            kind,
            &self.handle.stop,
            deadline,
        )
        .await?;
        self.output
            .send(kind, frame, &self.handle.stop, deadline)
            .await
    }

    pub(super) fn operation<T>(
        &self,
        operation: Operation,
        future: impl Future<Output = Result<T>>,
    ) -> impl Future<Output = Result<T>> {
        self.operation_until(operation, Instant::now() + STREAM_ANSWER_TIMEOUT, future)
    }

    pub(super) fn operation_until<T>(
        &self,
        operation: Operation,
        deadline: Instant,
        future: impl Future<Output = Result<T>>,
    ) -> impl Future<Output = Result<T>> {
        // Box the service computation before building the supervised future; every request stays small.
        let future = Box::pin(future);
        async move {
            let result = within(&self.handle.stop, deadline, operation, future).await;
            if let Err(error) = &result {
                crate::Gateway::observe(error, operation);
            }
            result
        }
    }

    pub(super) fn policy(beat: &Beat) -> Result<HeartbeatResponse<'_>> {
        Ok(HeartbeatResponse {
            status: HeartbeatStatus::Ok,
            assigned_policy: beat.assignment.decode()?,
            degraded: beat.verdict.is_degraded(),
            degraded_reason: beat.verdict.reason().map(Cow::Borrowed),
            selftest_requested: beat.selftest_requested,
            release_holds: beat
                .release_holds
                .iter()
                .map(|id| id.as_str().into())
                .collect(),
            heartbeat_interval_ms: super::opening::interval_millis()?,
        })
    }

    pub(super) async fn cancel(
        &self,
        lease: &af_core::id::Uuid7,
        code: &'static str,
    ) -> Result<()> {
        self.cache.cancel(lease);
        self.send(&Message::Cancel {
            lease_id: lease.as_str().into(),
            error_code: code.into(),
        })
        .await
    }

    pub(super) fn apply_policy(&self, beat: &Beat) {
        self.handle.apply_policy(beat.verdict.is_degraded());
    }

    pub(super) async fn recheck(&self) -> Result<()> {
        let beat = self
            .operation(Operation::Recheck, async {
                super::opening::allowed(
                    self.gateway
                        .inner
                        .dependencies
                        .runners
                        .recheck_connection(&self.authority)
                        .await?,
                )
            })
            .await?;
        self.apply_policy(&beat);
        self.send(&Message::Assignment {
            response: Self::policy(&beat)?,
        })
        .await?;
        self.refresh_cancellations().await
    }

    pub(super) async fn refresh_cancellations(&self) -> Result<()> {
        let cancelled = self
            .operation(
                Operation::Recheck,
                self.cache.refresh(
                    &self.gateway.inner.dependencies.database,
                    &self.handle.runner,
                ),
            )
            .await?;
        for lease in cancelled {
            self.cancel(&lease, af_core::error_code::RUN_LEASE_CANCELLED.as_str())
                .await?;
        }
        Ok(())
    }
}

pub(super) fn answer<T: Serialize>(result: Result<T>) -> Answer<'static, T> {
    match result {
        Ok(response) => Answer::Ok { response },
        Err(error) => Answer::Refused {
            error_code: error.code().as_str().into(),
            detail: error.detail().into_owned().into(),
            retryable: error.retryable(),
        },
    }
}
