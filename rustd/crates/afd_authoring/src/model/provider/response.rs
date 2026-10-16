//! Usage is authoritative before any untrusted text or proposal is interpreted.

use afd_credential::provider::RegisteredModel;
use afd_wire::authoring::{ENVELOPE_BYTES_MAX, ModelResult, ModelUsage, ProposalCall};
use garde::Validate as _;
use serde::Deserialize;
use serde_json::Value;

use super::super::{ModelCallInput, ModelCallResult, budget};
use super::{
    KEY_CALL_ID, KEY_CONTENT, KEY_TYPE, KIND_FUNCTION_CALL, ProviderResponse, prompt::TOOL,
};
use crate::{Result, error::ErrorKind};

/// A content part the model declined in; its text is the refusal, kept as text.
const KIND_REFUSAL: &str = "refusal";

#[derive(Deserialize)]
struct Envelope {
    status: String,
    #[serde(default)]
    output: Vec<Value>,
    usage: Option<Usage>,
}
#[derive(Deserialize)]
struct Usage {
    input_tokens: i64,
    output_tokens: i64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Arguments {
    proposal_json: String,
}

pub(in crate::model) struct Decoded {
    pub(in crate::model) result: Option<ModelCallResult>,
    pub(in crate::model) usage: ModelUsage,
    pub(in crate::model) input: i64,
    pub(in crate::model) output: i64,
}

pub(in crate::model) fn decode(
    response: &ProviderResponse,
    request: &garde::Valid<ModelCallInput>,
    model: &RegisteredModel,
) -> Result<Decoded> {
    if response.body.len() > ENVELOPE_BYTES_MAX {
        return Err(ErrorKind::Unknown.into());
    }
    let envelope: Envelope =
        serde_json::from_slice(&response.body).map_err(|_untrusted| ErrorKind::Unknown)?;
    let usage = envelope.usage.as_ref().ok_or(ErrorKind::Unknown)?;
    if usage.input_tokens < 0
        || usage.input_tokens > budget::INPUT_PER_CALL
        || usage.output_tokens < 0
        || usage.output_tokens > i64::from(request.request.max_output_tokens)
    {
        return Err(ErrorKind::Unknown.into());
    }
    let cost = budget::cost(model, usage.input_tokens, usage.output_tokens)?;
    let known = ModelUsage::Known {
        input_tokens: usage.input_tokens.cast_unsigned(),
        output_tokens: usage.output_tokens.cast_unsigned(),
        provider_cost_nanos: cost.to_string(),
    };
    let result = if (200..300).contains(&response.status) && envelope.status != "failed" {
        // Invalid private output is a known-usage provider failure. Keep its
        // accounting even when no text/proposal can be returned.
        content(&envelope, request, known.clone()).ok()
    } else {
        None
    };
    Ok(Decoded {
        result,
        usage: known,
        input: usage.input_tokens,
        output: usage.output_tokens,
    })
}

fn content(
    envelope: &Envelope,
    request: &garde::Valid<ModelCallInput>,
    usage: ModelUsage,
) -> Result<ModelCallResult> {
    let cut = envelope.status != "completed";
    let mut text = String::new();
    let mut proposal_calls = Vec::new();
    for item in &envelope.output {
        match item.get(KEY_TYPE).and_then(Value::as_str) {
            Some("message") => {
                append_text(item, &mut text)?;
            }
            Some(KIND_FUNCTION_CALL) if !cut => proposal_calls.push(proposal(item)?),
            Some(KIND_FUNCTION_CALL) => {}
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
        || result.text.contains('\0')
        || result.proposal_calls.len() > 8
    {
        return Err(ErrorKind::Provider.into());
    }
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

fn append_text(item: &Value, text: &mut String) -> Result<()> {
    let content = item
        .get(KEY_CONTENT)
        .and_then(Value::as_array)
        .ok_or(ErrorKind::Provider)?;
    for part in content {
        let kind = part.get(KEY_TYPE).and_then(Value::as_str);
        if !matches!(kind, Some("output_text" | KIND_REFUSAL)) {
            return Err(ErrorKind::Provider.into());
        }
        let content = part
            .get("text")
            .or_else(|| part.get(KIND_REFUSAL))
            .and_then(Value::as_str)
            .ok_or(ErrorKind::Provider)?;
        text.push_str(content);
    }
    Ok(())
}

fn proposal(
    item: &Value,
) -> Result<
    ProposalCall<
        afd_fleet_runtime::Definition,
        afd_fleet_runtime::definition::ConfigOp,
        afd_fleet_runtime::definition::InstructionEdit,
    >,
> {
    if item.get("name").and_then(Value::as_str) != Some(TOOL) {
        return Err(ErrorKind::Provider.into());
    }
    let id = item
        .get(KEY_CALL_ID)
        .and_then(Value::as_str)
        .ok_or(ErrorKind::Provider)?;
    let raw = item
        .get("arguments")
        .and_then(Value::as_str)
        .ok_or(ErrorKind::Provider)?;
    let arguments: Arguments =
        serde_json::from_str(raw).map_err(|_untrusted| ErrorKind::Provider)?;
    let proposal =
        serde_json::from_str(&arguments.proposal_json).map_err(|_untrusted| ErrorKind::Provider)?;
    let call = ProposalCall {
        id: id.into(),
        proposal,
    };
    call.validate().map_err(|_untrusted| ErrorKind::Provider)?;
    Ok(call)
}
