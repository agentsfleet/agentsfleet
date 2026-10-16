//! Translate the closed lease protocol to fixed Responses fields.

use super::super::ModelCallInput;
use super::prompt::{SYSTEM, TOOL, tool};
use super::{KEY_CALL_ID, KEY_CONTENT, KEY_TYPE, KIND_FUNCTION_CALL, PROPOSAL_ARGUMENT};
use crate::{Result, error::raise::stored};
use afd_credential::provider::RegisteredModel;
use afd_wire::authoring::ModelMessage;
use serde_json::{Value, json};

/// Who a conversation item speaks for.
const KEY_ROLE: &str = "role";

pub(in crate::model) fn body(
    request: &garde::Valid<ModelCallInput>,
    model: &RegisteredModel,
) -> Result<Value> {
    let mut input = Vec::new();
    for message in &request.request.messages {
        match message {
            ModelMessage::User { text } => input.push(json!({KEY_ROLE:"user",KEY_CONTENT:text})),
            ModelMessage::Assistant {
                text,
                proposal_calls,
            } => {
                if !text.is_empty() {
                    input.push(json!({KEY_ROLE:"assistant",KEY_CONTENT:text}));
                }
                for call in proposal_calls {
                    let proposal = serde_json::to_string(&call.proposal).map_err(stored)?;
                    let arguments = serde_json::to_string(&json!({PROPOSAL_ARGUMENT:proposal}))
                        .map_err(stored)?;
                    input.push(json!({KEY_TYPE:KIND_FUNCTION_CALL,KEY_CALL_ID:call.id,"name":TOOL,"arguments":arguments}));
                }
            }
            ModelMessage::Tool { call_id, text } => {
                input.push(
                    json!({KEY_TYPE:"function_call_output",KEY_CALL_ID:call_id,"output":text}),
                );
            }
        }
    }
    Ok(json!({
        "model": model.model, "instructions":SYSTEM, "input":input,
        "tools":[tool()], "tool_choice":"auto", "parallel_tool_calls":false,
        "max_output_tokens":request.request.max_output_tokens,
        "store":false, "stream":false, "truncation":"disabled"
    }))
}
