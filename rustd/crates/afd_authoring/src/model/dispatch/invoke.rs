//! Release reservation state before streaming; settle only after previews drain.

use super::{Intent, ModelCallInput, ModelCallResult, ModelCalls, guard, provider, receipt};
use crate::{Result, error::ErrorKind};
use afd_core::id::Uuid7;
use std::sync::Arc;

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
                    let mut decoded = if compaction.is_some() {
                        provider::decode_summary(
                            status,
                            &response,
                            request,
                            &attempt.model_snapshot,
                            output_limit,
                        )
                    } else {
                        provider::decode(
                            status,
                            &response,
                            request,
                            &attempt.model_snapshot,
                            output_limit,
                            candidate.as_ref(),
                        )
                    };
                    if let Ok(decoded) = decoded.as_mut() {
                        super::super::continuation::bind(decoded, &context_digest);
                    }
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
