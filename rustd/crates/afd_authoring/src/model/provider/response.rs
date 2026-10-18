//! Usage is authoritative before any untrusted text or proposal is interpreted.

use afd_credential::provider::RegisteredModel;
use afd_wire::authoring::{ENVELOPE_BYTES_MAX, ModelResult, ModelUsage, ProposalCall};
use garde::Validate as _;
use rig_core::{
    completion::{CompletionResponse, FinishReason},
    message::{AssistantContent, ToolCall},
};
use serde::Deserialize;

use super::super::{ModelCallInput, ModelCallResult, budget};
use super::prompt::TOOL;
use crate::{Result, error::ErrorKind};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Arguments {
    proposal_json: String,
}

pub(in crate::model) struct Decoded {
    pub(in crate::model) result: Option<ModelCallResult>,
    pub(in crate::model) continuation: Option<serde_json::Value>,
    pub(in crate::model) usage: ModelUsage,
    pub(in crate::model) output: i64,
}

pub(in crate::model) fn decode(
    status: u16,
    response: &CompletionResponse,
    request: &garde::Valid<ModelCallInput>,
    model: &RegisteredModel,
    output_limit: u32,
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
        content(response, request, known.clone())
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
    if input > budget::INPUT_PER_CALL
        || output > i64::from(output_limit)
        || input + output > i64::from(model.context_window_tokens)
    {
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
            AssistantContent::ToolCall(call) if !cut => proposal_calls.push(proposal(call)?),
            AssistantContent::ToolCall(_) | AssistantContent::Reasoning(_) => {}
            _unexpected => return Err(ErrorKind::Provider.into()),
        }
    }
    unique_calls(request, &proposal_calls)?;
    let result = ModelResult {
        dispatch_id: request.dispatch_id.clone(),
        call_index: request.call_index,
        replayed: false,
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
            used.extend(proposal_calls.iter().map(|call| call.id.as_str()));
        }
    }
    for call in calls {
        if !used.insert(call.id.as_str()) {
            return Err(ErrorKind::Provider.into());
        }
    }
    Ok(())
}

fn proposal(
    item: &ToolCall,
) -> Result<
    ProposalCall<
        afd_fleet_runtime::Definition,
        afd_fleet_runtime::definition::ConfigOp,
        afd_fleet_runtime::definition::InstructionEdit,
    >,
> {
    if item.function.name.as_str() != TOOL || item.function.invalid_arguments.is_some() {
        return Err(ErrorKind::Provider.into());
    }
    // A locally generated SDK id is not a provider acknowledgement.
    let id = item.id.provider().ok_or(ErrorKind::Provider)?.as_str();
    let arguments: Arguments =
        serde_json::from_value(serde_json::Value::Object(item.function.arguments.clone()))
            .map_err(|_untrusted| ErrorKind::Provider)?;
    let proposal =
        serde_json::from_str(&arguments.proposal_json).map_err(|_untrusted| ErrorKind::Provider)?;
    let call = ProposalCall {
        id: id.into(),
        proposal,
    };
    call.validate().map_err(|_untrusted| ErrorKind::Provider)?;
    Ok(call)
}
