//! A fresh native dispatch permits one provider call; receipts precede usable answers.

use super::FencedVerbs;
use crate::{Result, error};
use afd_core::id::Uuid7;
use afd_crypto::secret::SecretString;
use afd_wire::task::{
    DecisionCallState, DecisionDispatch, DecisionNotSentCause, DecisionReceipt, DecisionRecorded,
    DecisionReservationInput, DecisionReservationResult, DecisionRetryDelay,
    DecisionUnavailableCause,
};
use afr_providers::decision::{DecisionClient, DecisionEvaluator as _};
use afr_tools::verbs::DecisionEvaluation;

impl FencedVerbs<'_> {
    pub(super) async fn evaluate(
        &self,
        mut input: DecisionReservationInput,
    ) -> Result<DecisionEvaluation> {
        input.fencing_token = self.fencing_token;
        // Native code permits only one 429/529 retry and retains the original
        // quote, evidence and 30-second logical deadline across both attempts.
        for attempt in 0..2 {
            let body = self
                .plane
                .task_decision_reserve(self.lease_id, &input)
                .await?;
            let (evaluated, retry) = match body.decode::<DecisionReservationResult<'_>>()? {
                DecisionReservationResult::Existing {
                    call_id,
                    state,
                    recorded,
                } => {
                    return Ok(DecisionEvaluation {
                        call_id,
                        state,
                        recorded,
                    });
                }
                DecisionReservationResult::Dispatch { call_id, dispatch } => {
                    self.dispatch(call_id, *dispatch).await?
                }
            };
            let Some((delay, expires_at)) =
                retry.filter(|_| attempt == 0 && input.retry_of.is_none())
            else {
                return Ok(evaluated);
            };
            if expires_at <= afd_core::clock::now()
                || delay >= crate::deadline::left(afd_core::clock::now(), expires_at)
            {
                return Ok(evaluated);
            }
            tokio::select! {
                biased;
                () = self.stop.cancelled() => return Ok(evaluated),
                () = tokio::time::sleep(delay) => {},
            }
            if expires_at <= afd_core::clock::now() {
                return Ok(evaluated);
            }
            input.retry_of = Some(evaluated.call_id);
            input.operation_id = afd_crypto::entropy::Entropy::new()
                .uuid7(afd_core::clock::now())?
                .to_string();
        }
        Err(error::config("decision retry bound exceeded"))
    }

    async fn task_activity(&self, frame: afd_wire::activity::ActivityFrame<'static>) {
        let request = afd_wire::activity::ActivityRequest {
            frames: vec![frame],
        };
        // Progress cannot lengthen the decision's physical/logical deadline.
        let _sent = tokio::time::timeout(
            std::time::Duration::from_millis(500),
            self.plane.activity(self.lease_id, &request),
        )
        .await;
    }

    async fn dispatch(
        &self,
        call_id: String,
        dispatch: DecisionDispatch<'_>,
    ) -> Result<(
        DecisionEvaluation,
        Option<(std::time::Duration, jiff::Timestamp)>,
    )> {
        let call = Uuid7::parse(&call_id)?;
        if dispatch.request.call_id != call_id {
            return Err(error::config(
                "native decision dispatch names another physical call",
            ));
        }
        // Failure can mean an existing call directory, including an earlier
        // physical send. It cannot prove that this native identity was unsent.
        let pending = self
            .spool
            .begin_decision(self.lease_id, &call, self.fencing_token)
            .await?;
        let request = dispatch.request.clone();
        let mut progress = afd_wire::activity::TaskActivity {
            activity_id: call_id.clone(),
            task_id: request
                .state
                .get("task")
                .and_then(|task| task.get("task_id"))
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned),
            source_admission_id: request
                .state
                .get("source_admission_id")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned),
            operation: if request.purpose == afd_wire::decision::DecisionPurpose::Goal {
                afd_wire::activity::TaskActivityOperation::Goal
            } else {
                afd_wire::activity::TaskActivityOperation::Intent
            },
            phase: afd_wire::activity::TaskActivityPhase::Starting,
            provider: Some(
                match dispatch.assignment.profile.api {
                    afd_wire::decision::DecisionApi::Typesafe => "typesafe",
                    afd_wire::decision::DecisionApi::Openai => "openai",
                    afd_wire::decision::DecisionApi::Perplexity => "perplexity",
                    afd_wire::decision::DecisionApi::Cloudflare => "cloudflare",
                }
                .to_owned(),
            ),
            model: Some(dispatch.assignment.profile.model.clone()),
            completed: 0,
            total: None,
            bytes: None,
            sequence: None,
        };
        self.task_activity(afd_wire::activity::ActivityFrame::TaskDecisionStarted(
            progress.clone(),
        ))
        .await;
        let expires_at = dispatch.expires_at;
        let attempt = dispatch.attempt;
        let receipt = physical(dispatch, self.stop).await;
        let retry = retry_delay(&receipt)
            .filter(|_| attempt == 0)
            .map(|delay| (delay, expires_at));
        let recorded = match &receipt {
            DecisionReceipt::Provider { result } => Some(result.clone()),
            DecisionReceipt::Unknown { .. } | DecisionReceipt::NotSent { .. } => None,
        };
        let (pending, saved) = pending.finish(receipt).await?;
        if let Err(failure) = &saved {
            tracing::warn!(event = "decision_receipt_storage_failed", error_code = %failure.code());
        }
        let result = pending.deliver(self.plane).await;
        self.held.notify_one();
        // A disk failure still permits immediate original-owner accounting.
        // No answer reaches the root unless the daemon acknowledges it.
        let result = result?;
        progress.phase = if result.state == DecisionCallState::Known {
            afd_wire::activity::TaskActivityPhase::Recorded
        } else {
            afd_wire::activity::TaskActivityPhase::Unavailable
        };
        self.task_activity(afd_wire::activity::ActivityFrame::TaskDecisionCompleted(
            progress,
        ))
        .await;
        let retry = retry.filter(|_| result.state == DecisionCallState::Unknown);
        Ok((
            DecisionEvaluation {
                call_id,
                state: result.state,
                recorded: recorded
                    .filter(|_| result.state == DecisionCallState::Known)
                    .map(|result_body| {
                        Box::new(DecisionRecorded {
                            request,
                            result: result_body,
                            disposition: result.disposition,
                        })
                    }),
            },
            retry,
        ))
    }
}

async fn physical(
    dispatch: DecisionDispatch<'_>,
    stop: &tokio_util::sync::CancellationToken,
) -> DecisionReceipt {
    let credential = SecretString::new(dispatch.api_key.into_owned());
    let now = afd_core::clock::now();
    if stop.is_cancelled() || dispatch.expires_at <= now {
        return DecisionReceipt::NotSent {
            cause: DecisionNotSentCause::Preflight,
        };
    }
    let Ok(profile) = garde::Unvalidated::new(dispatch.assignment.profile).validate() else {
        return DecisionReceipt::NotSent {
            cause: DecisionNotSentCause::Preflight,
        };
    };
    let Ok(request) = garde::Unvalidated::new(dispatch.request).validate() else {
        return DecisionReceipt::NotSent {
            cause: DecisionNotSentCause::Preflight,
        };
    };
    let Ok(client) = DecisionClient::new(profile, credential) else {
        return DecisionReceipt::NotSent {
            cause: DecisionNotSentCause::Build,
        };
    };
    let now = afd_core::clock::now();
    if stop.is_cancelled() || dispatch.expires_at <= now {
        return DecisionReceipt::NotSent {
            cause: DecisionNotSentCause::Preflight,
        };
    }
    let result = tokio::select! {
        biased;
        () = stop.cancelled() => return DecisionReceipt::Unknown { cause: DecisionUnavailableCause::Transport },
        result = tokio::time::timeout(crate::deadline::left(now, dispatch.expires_at), client.evaluate(&request)) => result,
    };
    match result {
        Ok(Ok(result)) => DecisionReceipt::Provider { result },
        Ok(Err(failure)) if failure.decision_not_sent() => DecisionReceipt::NotSent {
            cause: DecisionNotSentCause::Preflight,
        },
        Ok(Err(failure)) => DecisionReceipt::Unknown {
            cause: unavailable(&failure),
        },
        Err(_) => DecisionReceipt::Unknown {
            cause: DecisionUnavailableCause::Timeout,
        },
    }
}

fn unavailable(failure: &afr_providers::Error) -> DecisionUnavailableCause {
    match failure.decision_status() {
        Some(status) => {
            let retry_delay = match failure.decision_retry_after() {
                None => DecisionRetryDelay::Unspecified,
                Some(delay) if delay <= std::time::Duration::from_secs(2) => {
                    DecisionRetryDelay::WithinBudget {
                        milliseconds: u16::try_from(delay.as_millis()).unwrap_or(2000),
                    }
                }
                Some(_) => DecisionRetryDelay::TooLong,
            };
            DecisionUnavailableCause::ProviderStatus {
                status,
                retry_delay,
            }
        }
        None => DecisionUnavailableCause::Transport,
    }
}

fn retry_delay(receipt: &DecisionReceipt) -> Option<std::time::Duration> {
    let DecisionReceipt::Unknown {
        cause:
            DecisionUnavailableCause::ProviderStatus {
                status: 429 | 529,
                retry_delay,
            },
    } = receipt
    else {
        return None;
    };
    match retry_delay {
        DecisionRetryDelay::Unspecified => Some(std::time::Duration::from_millis(250)),
        DecisionRetryDelay::WithinBudget { milliseconds } => {
            Some(std::time::Duration::from_millis(u64::from(*milliseconds)))
        }
        DecisionRetryDelay::TooLong => None,
    }
}
