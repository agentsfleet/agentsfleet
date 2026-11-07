//! One record per evaluation, including a caller dropping the in-flight future.

use std::time::Instant;

use af_wire::decision::{
    DecisionApi, DecisionInvalid, DecisionOutcome as AnswerOutcome, DecisionProfile,
    DecisionPurpose as Purpose, DecisionRequest, DecisionResult,
};
use afr_telemetry::{
    labels::decision::{DecisionOutcome, DecisionProvider, DecisionPurpose},
    record,
};
use garde::Valid;
use tracing::Instrument as _;

use super::DecisionClient;
use crate::{Result, error::ErrorKind};

struct Observation<'a> {
    call_id: &'a str,
    provider: DecisionProvider,
    purpose: DecisionPurpose,
    started: Instant,
    finished: bool,
}

impl Observation<'_> {
    fn finish(&mut self, result: &Result<DecisionResult>) {
        self.finished = true;
        let outcome = outcome(result);
        let elapsed = self.started.elapsed();
        record::decision(self.provider, self.purpose, outcome, elapsed);
        let outcome = outcome.as_str();
        let elapsed_seconds = elapsed.as_secs_f64();
        let decision_call_id = self.call_id;
        match result {
            Ok(_) => tracing::info!(
                event = "dt_decision_transport_completed",
                decision_call_id,
                outcome,
                elapsed_seconds
            ),
            Err(error) => {
                let error_code = error.code().as_str();
                tracing::warn!(
                    event = "dt_decision_transport_failed",
                    decision_call_id,
                    outcome,
                    elapsed_seconds,
                    error_code
                );
            }
        }
    }
}

impl Drop for Observation<'_> {
    fn drop(&mut self) {
        if !self.finished {
            let elapsed = self.started.elapsed();
            record::decision(
                self.provider,
                self.purpose,
                DecisionOutcome::Cancelled,
                elapsed,
            );
            let decision_call_id = self.call_id;
            let elapsed_seconds = elapsed.as_secs_f64();
            let error_code = af_core::error_code::INTERNAL_OPERATION_FAILED.as_str();
            tracing::warn!(
                event = "dt_decision_transport_failed",
                decision_call_id,
                elapsed_seconds,
                error_code,
                outcome = "cancelled"
            );
        }
    }
}

pub(super) async fn evaluate(
    client: &DecisionClient,
    profile: &Valid<DecisionProfile>,
    request: &Valid<DecisionRequest>,
    credential: &af_crypto::secret::SecretString,
) -> Result<DecisionResult> {
    let provider = match profile.api {
        DecisionApi::Typesafe => DecisionProvider::Typesafe,
        DecisionApi::Openai => DecisionProvider::Openai,
        DecisionApi::Perplexity => DecisionProvider::Perplexity,
        DecisionApi::Cloudflare => DecisionProvider::Cloudflare,
    };
    let purpose = match request.purpose {
        Purpose::Intent => DecisionPurpose::Intent,
        Purpose::Goal => DecisionPurpose::Goal,
    };
    let span = afr_telemetry::decision::span(
        &request.call_id,
        &profile.model_id,
        &profile.revision,
        provider,
        purpose,
    );
    async {
        let decision_call_id = request.call_id.as_str();
        let mut observation = Observation {
            call_id: decision_call_id,
            provider,
            purpose,
            started: Instant::now(),
            finished: false,
        };
        tracing::info!(event = "dt_decision_transport_started", decision_call_id);
        let result = client.send(profile, request, credential).await;
        observation.finish(&result);
        result
    }
    .instrument(span)
    .await
}

fn outcome(result: &Result<DecisionResult>) -> DecisionOutcome {
    match result {
        Ok(result) => match result.result {
            AnswerOutcome::Answered { .. } => DecisionOutcome::Completed,
            AnswerOutcome::Refused => DecisionOutcome::Refused,
            AnswerOutcome::Invalid {
                reason: DecisionInvalid::Model,
            } => DecisionOutcome::InvalidModel,
            AnswerOutcome::Invalid {
                reason: DecisionInvalid::Usage,
            } => DecisionOutcome::InvalidUsage,
            AnswerOutcome::Invalid {
                reason: DecisionInvalid::Answers,
            } => DecisionOutcome::InvalidAnswers,
        },
        Err(error) => match error.kind() {
            ErrorKind::DecisionEndpoint { .. }
            | ErrorKind::DecisionBuild { .. }
            | ErrorKind::DecisionPreflight { .. }
            | ErrorKind::Configuration { .. } => DecisionOutcome::NotSent,
            ErrorKind::DecisionStatus {
                status: 429 | 529, ..
            } => DecisionOutcome::RateLimited,
            ErrorKind::DecisionStatus { .. } => DecisionOutcome::ProviderFailure,
            ErrorKind::DecisionOversize => DecisionOutcome::Oversize,
            ErrorKind::Unreadable { .. } => DecisionOutcome::Malformed,
            ErrorKind::Lost { source }
                if source
                    .downcast_ref::<reqwest::Error>()
                    .is_some_and(reqwest::Error::is_timeout) =>
            {
                DecisionOutcome::Timeout
            }
            _ => DecisionOutcome::Unknown,
        },
    }
}
