//! Usage is authoritative before any untrusted text or proposal is interpreted.

use afd_credential::provider::RegisteredModel;
use afd_wire::authoring::{
    ENVELOPE_BYTES_MAX, ModelResult, ModelUsage, ProposalCall, ProposalRejection,
};
use garde::Validate as _;
use rig_core::{
    completion::{CompletionResponse, FinishReason},
    message::{AssistantContent, ToolCall},
};
use serde::Deserialize;

use super::super::{ModelCallInput, ModelCallResult, budget};
use super::prompt::TOOL;
use crate::{Result, error::ErrorKind};

#[cfg(test)]
#[path = "tests/proposals.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/corrections.rs"]
mod corrections;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Arguments {
    proposal: super::edits::Suggestion,
}

pub(in crate::model) struct Decoded {
    pub(in crate::model) result: Option<ModelCallResult>,
    pub(in crate::model) continuation: Option<serde_json::Value>,
    pub(in crate::model) usage: ModelUsage,
    pub(in crate::model) output: i64,
    pub(in crate::model) summary: Option<afd_context::Summary>,
}

impl Decoded {
    pub(in crate::model) fn rejected() -> Self {
        Self {
            summary: None,
            result: None,
            continuation: None,
            output: 0,
            usage: ModelUsage::Known {
                input_tokens: 0,
                output_tokens: 0,
                provider_cost_nanos: 0.to_string(),
            },
        }
    }
}

pub(in crate::model) fn decode(
    status: u16,
    response: &CompletionResponse,
    request: &garde::Valid<ModelCallInput>,
    model: &RegisteredModel,
    output_limit: u32,
    candidate: Option<&crate::types::Candidate>,
) -> Result<Decoded> {
    let (known, output) = usage(response, model, output_limit)?;
    // Rig names an incomplete response without a reason as an error. It still
    // reports authoritative usage; return a cut turn with no executable tools.
    let incomplete = matches!(response.finish_reason(), Some(FinishReason::Other(reason)) if reason == "incomplete");
    let result = if (200..300).contains(&status)
        && (response.error.is_none() || incomplete)
        && response.aborted.is_none()
    {
        // Invalid private output retains known usage without executable tools.
        content(response, request, known.clone(), candidate)
            .and_then(|result| {
                let continuation = super::super::continuation::save(response, request, &result)?;
                Ok((result, continuation))
            })
            .ok()
    } else {
        None
    };
    let (result, continuation) = result.map_or((None, None), |(result, continuation)| {
        (Some(result), continuation)
    });
    Ok(Decoded {
        summary: None,
        result,
        continuation,
        usage: known,
        output,
    })
}

fn usage(
    response: &CompletionResponse,
    model: &RegisteredModel,
    output_limit: u32,
) -> Result<(ModelUsage, i64)> {
    let input = response
        .usage
        .input_tokens
        .and_then(|value| i64::try_from(value).ok())
        .ok_or(ErrorKind::Unknown)?;
    let output = response
        .usage
        .output_tokens
        .and_then(|value| i64::try_from(value).ok())
        .ok_or(ErrorKind::Unknown)?;
    if output > i64::from(output_limit) || input + output > i64::from(model.context_window_tokens) {
        return Err(ErrorKind::Unknown.into());
    }
    let cost = budget::cost(model, input, output)?;
    let known = ModelUsage::Known {
        input_tokens: input.cast_unsigned(),
        output_tokens: output.cast_unsigned(),
        provider_cost_nanos: cost.to_string(),
    };
    Ok((known, output))
}

fn content(
    response: &CompletionResponse,
    request: &garde::Valid<ModelCallInput>,
    usage: ModelUsage,
    candidate: Option<&crate::types::Candidate>,
) -> Result<ModelCallResult> {
    let cut = !matches!(
        response.finish_reason(),
        Some(FinishReason::Stop | FinishReason::ToolCalls)
    );
    let mut text = String::new();
    let mut proposal_calls = Vec::new();
    for item in &response.choice {
        match item {
            AssistantContent::Text(part) => text.push_str(&part.text),
            AssistantContent::ToolCall(call) if !cut => {
                proposal_calls.push(proposal(call, candidate)?);
            }
            AssistantContent::ToolCall(_) | AssistantContent::Reasoning(_) => {}
            _unexpected => return Err(ErrorKind::Provider.into()),
        }
    }
    unique_calls(request, &proposal_calls)?;
    let result = ModelResult {
        dispatch_id: request.dispatch_id.clone(),
        call_index: request.call_index,
        replayed: false,
        compacted: false,
        text,
        proposal_calls,
        usage,
        cut,
    };
    if serde_json::to_vec(&result)
        .map_err(|_untrusted| ErrorKind::Provider)?
        .len()
        > ENVELOPE_BYTES_MAX
    {
        return Err(ErrorKind::Provider.into());
    }
    result
        .validate()
        .map_err(|_untrusted| ErrorKind::Provider)?;
    Ok(result)
}

fn unique_calls(
    request: &garde::Valid<ModelCallInput>,
    calls: &[ProposalCall<
        afd_fleet_runtime::Definition,
        afd_fleet_runtime::definition::ConfigOp,
        afd_fleet_runtime::definition::InstructionEdit,
    >],
) -> Result<()> {
    let mut used = std::collections::HashSet::new();
    for message in &request.request.messages {
        if let afd_wire::authoring::ModelMessage::Assistant { proposal_calls, .. } = message {
            used.extend(proposal_calls.iter().map(ProposalCall::id));
        }
    }
    for call in calls {
        if !used.insert(call.id()) {
            return Err(ErrorKind::Provider.into());
        }
    }
    Ok(())
}

fn proposal(
    item: &ToolCall,
    candidate: Option<&crate::types::Candidate>,
) -> Result<
    ProposalCall<
        afd_fleet_runtime::Definition,
        afd_fleet_runtime::definition::ConfigOp,
        afd_fleet_runtime::definition::InstructionEdit,
    >,
> {
    if item.function.name.as_str() != TOOL {
        return Err(ErrorKind::Provider.into());
    }
    // A locally generated SDK id is not a provider acknowledgement.
    let id = item.id.provider().ok_or(ErrorKind::Provider)?.as_str();
    let converted = if item.function.invalid_arguments.is_some() {
        Err(ProposalRejection::InvalidArguments)
    } else {
        serde_json::from_value::<Arguments>(serde_json::Value::Object(
            item.function.arguments.clone(),
        ))
        .map_err(|_untrusted| ProposalRejection::InvalidArguments)
        .and_then(|arguments| super::edits::native(arguments.proposal, candidate))
    };
    let call = match converted {
        Ok(proposal) => ProposalCall::Proposed {
            id: id.into(),
            proposal,
        },
        Err(reason) => ProposalCall::Refused {
            id: id.into(),
            reason,
        },
    };
    call.validate().map_err(|_untrusted| ErrorKind::Provider)?;
    Ok(call)
}

/// A summary is usable only after complete stop and strict notes validation.
pub(in crate::model) fn decode_summary(
    status: u16,
    response: &CompletionResponse,
    request: &garde::Valid<ModelCallInput>,
    model: &RegisteredModel,
    output_limit: u32,
) -> Result<Decoded> {
    let (known, output) = usage(response, model, output_limit)?;
    let mut text = String::new();
    let mut usable = (200..300).contains(&status)
        && response.error.is_none()
        && response.aborted.is_none()
        && matches!(response.finish_reason(), Some(FinishReason::Stop));
    for item in &response.choice {
        match item {
            AssistantContent::Text(part) => text.push_str(&part.text),
            AssistantContent::Reasoning(_) => {}
            _ => usable = false,
        }
    }
    let summary = usable
        .then(|| afd_context::Summary::decode(&text).ok())
        .flatten();
    let result = summary.as_ref().map(|_| ModelResult {
        dispatch_id: request.dispatch_id.clone(),
        call_index: request.call_index,
        replayed: false,
        compacted: true,
        text: String::new(),
        proposal_calls: Vec::new(),
        usage: known.clone(),
        cut: false,
    });
    Ok(Decoded {
        result,
        continuation: None,
        usage: known,
        output,
        summary,
    })
}
