//! Thin protocol adapters; one shared validator owns answer interpretation.

mod native;
mod openai;
mod usage;

use afd_wire::decision::{
    DecisionApi, DecisionInvalid, DecisionOutcome, DecisionProfile, DecisionRequest, DecisionResult,
};
use garde::Valid;
use serde_json::Value;

use super::document::Document;
use crate::{Result, error::ErrorKind};

pub(super) fn encode(
    profile: &Valid<DecisionProfile>,
    request: &Valid<DecisionRequest>,
) -> Result<Vec<u8>> {
    let document = match profile.api {
        DecisionApi::Openai => openai::encode(profile, request)?,
        DecisionApi::Typesafe | DecisionApi::Perplexity | DecisionApi::Cloudflare => {
            native::encode(profile, request)?
        }
    };
    let body = serde_json::to_vec(&document)?;
    if body.len() > profile.input_max_bytes as usize {
        return Err(ErrorKind::DecisionPreflight {
            reason: "the complete encoded decision input exceeds the reviewed profile capacity",
        }
        .into());
    }
    Ok(body)
}

fn bounded_question(question: Value) -> Result<Value> {
    if !afd_wire::decision::serialized_within(&question, afd_wire::decision::QUESTION_MAX_BYTES) {
        return Err(ErrorKind::DecisionPreflight {
            reason: "an encoded decision question exceeds its byte bound",
        }
        .into());
    }
    Ok(question)
}

pub(super) fn decode(
    profile: &Valid<DecisionProfile>,
    request: &Valid<DecisionRequest>,
    parsed: &Document,
) -> DecisionResult {
    let document = &parsed.value;
    let refused = profile.api == DecisionApi::Cloudflare
        && document.get("success").and_then(Value::as_bool) != Some(true);
    let document = if profile.api == DecisionApi::Cloudflare {
        document.get("result").unwrap_or(&Value::Null)
    } else {
        document
    };
    let reported_model = document
        .get("model")
        .and_then(Value::as_str)
        .filter(|_| !parsed.model_ambiguous)
        .map(str::to_owned);
    let usage = if parsed.usage_ambiguous {
        None
    } else {
        usage::decode(document.get("usage"))
    };
    let result = if refused {
        DecisionOutcome::Refused
    } else if reported_model
        .as_ref()
        .is_none_or(|model| !profile.accepted_models.contains(model))
    {
        DecisionOutcome::Invalid {
            reason: DecisionInvalid::Model,
        }
    } else if usage.is_none() {
        DecisionOutcome::Invalid {
            reason: DecisionInvalid::Usage,
        }
    } else if parsed.ambiguous {
        DecisionOutcome::Invalid {
            reason: DecisionInvalid::Answers,
        }
    } else {
        let answers = document.get("answers").unwrap_or(&Value::Null);
        match profile.api {
            DecisionApi::Openai => openai::answers(request, answers),
            DecisionApi::Typesafe | DecisionApi::Perplexity | DecisionApi::Cloudflare => {
                native::answers(request, answers)
            }
        }
    };
    DecisionResult {
        reported_model,
        usage,
        result,
    }
}
