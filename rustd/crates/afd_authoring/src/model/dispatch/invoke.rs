//! Release reservation state before streaming; settle only after previews drain.

use super::{Intent, ModelCallInput, ModelCallResult, ModelCalls, guard, provider, receipt};
use crate::{Result, error::ErrorKind};
use afd_core::id::Uuid7;
use afd_wire::context::ContextState;
use std::sync::Arc;
use tracing::Instrument as _;

impl ModelCalls {
    pub(super) async fn invoke(
        &self,
        runner: &Uuid7,
        lease: &Uuid7,
        request: &garde::Valid<ModelCallInput>,
        counters: &guard::Counters,
        intent: Intent,
        dispatch: Uuid7,
    ) -> Result<ModelCallResult> {
        let generation = intent
            .compaction
            .as_ref()
            .map(|task| task.expected_generation.saturating_add(1));
        let mut trace = intent.compaction.as_ref().map(|task| {
            afd_context::Trace::start(dispatch.as_str().into(), afd_context::Host::Authoring, task)
        });
        let span = trace
            .as_ref()
            .map_or_else(tracing::Span::none, |trace| trace.span().clone());
        let progress = self
            .streams
            .as_ref()
            .zip(generation)
            .map(|(streams, generation)| {
                crate::model::progress::ContextPublisher::new(
                    streams,
                    &self.drafts,
                    runner,
                    lease,
                    counters,
                    dispatch.clone(),
                    generation,
                )
            });
        if let Some(progress) = &progress {
            progress.send(ContextState::Summarizing).await;
        }
        let result = self
            .send(runner, lease, request, counters, intent, dispatch)
            .instrument(span)
            .await;
        if let Some(trace) = &mut trace {
            match &result {
                Ok(_) => trace.completed(generation.unwrap_or_default()),
                Err(error) => trace.failed(error.code()),
            }
        }
        if let Some(progress) = progress {
            progress
                .send(if result.is_ok() {
                    ContextState::Resuming
                } else {
                    ContextState::Interrupted
                })
                .await;
        }
        result
    }

    async fn send(
        &self,
        runner: &Uuid7,
        lease: &Uuid7,
        request: &garde::Valid<ModelCallInput>,
        counters: &guard::Counters,
        intent: Intent,
        dispatch: Uuid7,
    ) -> Result<ModelCallResult> {
        let Intent {
            attempt,
            mut outbound,
            authorized_at,
            output_limit,
            candidate,
            compaction,
            context_digest,
        } = intent;
        outbound.timeout = outbound.timeout.saturating_sub(authorized_at.elapsed());
        let publisher = self.preview(
            runner,
            lease,
            counters,
            dispatch,
            &mut outbound,
            compaction.is_none(),
        );
        let (decoded, cause) = if outbound.timeout.is_zero() {
            drop(outbound);
            (
                Err(ErrorKind::Unknown.into()),
                Some(crate::types::FailureCause::Deadline),
            )
        } else {
            match provider::complete(outbound, Arc::clone(&self.transport)).await {
                Ok((status, response, cause)) => {
                    let decoded = Interpretation {
                        request,
                        attempt: &attempt,
                        output_limit,
                        candidate: candidate.as_ref(),
                        compaction: compaction.is_some(),
                        context_digest: &context_digest,
                    }
                    .decode(status, &response);
                    received(decoded, cause)
                }
                Err(error) => (Err(error), None),
            }
        };
        if let Some(publisher) = publisher {
            publisher.drain().await;
        }
        receipt::finish(
            &self.drafts,
            runner,
            lease,
            request,
            counters,
            &attempt,
            receipt::Received {
                decoded,
                compaction,
                cause,
            },
        )
        .await
    }

    fn preview(
        &self,
        runner: &Uuid7,
        lease: &Uuid7,
        counters: &guard::Counters,
        dispatch: Uuid7,
        outbound: &mut provider::Outbound,
        enabled: bool,
    ) -> Option<crate::model::progress::Publisher> {
        self.streams.as_ref().filter(|_| enabled).map(|streams| {
            let (publisher, preview) = crate::model::progress::Publisher::start(
                streams.clone(),
                self.drafts.clone(),
                runner.clone(),
                lease.clone(),
                counters.clone(),
                dispatch,
            );
            outbound.progress = Some(preview);
            publisher
        })
    }
}

struct Interpretation<'a> {
    request: &'a garde::Valid<ModelCallInput>,
    attempt: &'a guard::Attempt,
    output_limit: u32,
    candidate: Option<&'a crate::types::Candidate>,
    compaction: bool,
    context_digest: &'a str,
}

impl Interpretation<'_> {
    fn decode(
        &self,
        status: u16,
        response: &rig_core::completion::CompletionResponse,
    ) -> Result<provider::Decoded> {
        let mut decoded = if self.compaction {
            provider::decode_summary(
                status,
                response,
                self.request,
                &self.attempt.model_snapshot,
                self.output_limit,
            )
        } else {
            provider::decode(
                status,
                response,
                self.request,
                &self.attempt.model_snapshot,
                self.output_limit,
                self.candidate,
            )
        }?;
        super::super::continuation::bind(&mut decoded, self.context_digest);
        Ok(decoded)
    }
}

fn received(
    decoded: Result<provider::Decoded>,
    cause: Option<crate::types::FailureCause>,
) -> (
    Result<provider::Decoded>,
    Option<crate::types::FailureCause>,
) {
    let cause = cause.or_else(|| {
        decoded
            .is_err()
            .then_some(crate::types::FailureCause::CompletionUnverified)
    });
    (decoded, cause)
}
