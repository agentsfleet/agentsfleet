//! Translate the closed lease protocol into the SDK's typed conversation.

use super::super::ModelCallInput;
use super::PROPOSAL_ARGUMENT;
use super::prompt::{TOOL, tool};
use crate::{
    Result,
    error::{ErrorKind, raise::stored},
};
use afd_credential::provider::RegisteredModel;
use afd_wire::{authoring::ModelMessage, inference::InferenceApi};
use rig_core::completion::{CompletionRequest, ToolAnnotations, ToolDefinition};
use rig_core::message::{
    AssistantContent, CallId, Message, ToolCall, ToolFunction, ToolName, UserContent,
};
use serde_json::json;

pub(in crate::model) fn body(
    request: &garde::Valid<ModelCallInput>,
    model: &RegisteredModel,
) -> Result<CompletionRequest> {
    let name = ToolName::new(TOOL).map_err(|_invalid| ErrorKind::Model)?;
    let mut history = Vec::with_capacity(request.request.messages.len());
    for message in &request.request.messages {
        history.push(message_to_sdk(message, &name)?);
    }
    let schema = tool();
    let mut body = CompletionRequest::new(Message::user(String::new()))
        .model(model.model.as_ref())
        .tools(vec![ToolDefinition {
            annotations: ToolAnnotations::default(),
            name,
            description: schema
                .get("description")
                .and_then(serde_json::Value::as_str)
                .ok_or(ErrorKind::Model)?
                .into(),
            parameters: schema.get("parameters").ok_or(ErrorKind::Model)?.clone(),
        }])
        .max_tokens(u64::from(request.request.max_output_tokens));
    body.chat_history = history.into_iter().map(std::sync::Arc::new).collect();
    if model
        .inference
        .as_ref()
        .is_some_and(|settings| settings.api == InferenceApi::Responses)
    {
        body.additional_params =
            Some(json!({"store":false,"parallel_tool_calls":false,"truncation":"disabled"}));
    }
    Ok(body)
}

fn message_to_sdk(
    message: &ModelMessage<
        afd_fleet_runtime::Definition,
        afd_fleet_runtime::definition::ConfigOp,
        afd_fleet_runtime::definition::InstructionEdit,
    >,
    tool: &ToolName,
) -> Result<Message> {
    Ok(match message {
        ModelMessage::User { text } => Message::user(text.clone()),
        ModelMessage::Assistant {
            text,
            proposal_calls,
        } => {
            let mut content = Vec::new();
            if !text.is_empty() {
                content.push(AssistantContent::text(text.clone()));
            }
            for call in proposal_calls {
                let proposal = serde_json::to_string(&call.proposal).map_err(stored)?;
                content.push(AssistantContent::ToolCall(ToolCall::new(
                    CallId::from_wire(call.id.clone()),
                    ToolFunction::new(tool.clone(), json!({PROPOSAL_ARGUMENT:proposal})),
                )));
            }
            Message::Assistant(rig_core::message::AssistantMessage::new(content))
        }
        ModelMessage::Tool { call_id, text } => Message::User {
            content: vec![UserContent::tool_result(
                CallId::from_wire(call_id.clone()),
                tool.clone(),
                vec![rig_core::message::ToolResultContent::text(text.clone())],
            )],
        },
    })
}
