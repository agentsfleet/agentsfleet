//! One turn's request in rig's terms.
//!
//! The system prompt leads, then the conversation, with each call's id kept so
//! its result answers it, then the tools, and the hosted ones as the wire's own
//! spec. Two things rig needs that the seam does not carry are rebuilt here,
//! after `IronClaw`'s adapter (`ironclaw_llm/src/rig_adapter.rs`): a tool
//! result's tool name, taken from the call it answers, and the reasoning a
//! provider asked to have back, ahead of the turn's text and calls.

use std::collections::HashMap;

use rig_core::completion::{
    AssistantMessage, CacheRetention, CompletionRequest, ProviderToolDefinition, ToolDefinition,
};
use rig_core::message::{
    AssistantContent, CallId, Message as RigMessage, ToolCall, ToolFunction, ToolName,
    ToolResultContent, UserContent,
};

use crate::error::{Result, raise};
use crate::image_input::{self, ImageInput};
use crate::provider::{Call, Hosted, Message, Request};
use crate::registry::Wire;

/// Anthropic's server-side web search, as its tool spec names it.
const WEB_SEARCH_MESSAGES: &str = "web_search_20250305";
/// The key a Messages server tool is named under.
const FIELD_NAME: &str = "name";
/// The Responses field naming a conversation's prompt cache.
const FIELD_PROMPT_CACHE_KEY: &str = "prompt_cache_key";

/// `request` as rig sends it over `wire`.
///
/// # Errors
/// A call names no tool, or a result answers no call earlier in the
/// conversation.
pub(crate) fn request(wire: Wire, request: &Request<'_>) -> Result<CompletionRequest> {
    let history = Conversation::default().map(request.messages)?;
    let tools = request
        .tools
        .iter()
        .map(|spec| {
            Ok(ToolDefinition {
                name: ToolName::new(spec.name).map_err(raise::nameless_tool)?,
                description: spec.description.to_owned(),
                parameters: spec.parameters.clone(),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let hosted = request
        .hosted
        .iter()
        .copied()
        .filter_map(|tool| hosted(wire, tool));
    let mut built = CompletionRequest::new(RigMessage::user(String::new()));
    built.chat_history = history;
    match wire {
        // Messages keeps the top-level breakpoint the provider moves along the
        // conversation, beside the markers on its static prefix.
        Wire::Messages => built = built.cache(CacheRetention::Short),
        Wire::Responses => {
            built.additional_params =
                Some(serde_json::json!({ FIELD_PROMPT_CACHE_KEY: request.cache_key }));
        }
        // The Chat wire's gateways refuse a field they do not know.
        Wire::Chat | Wire::Gemini | Wire::Ollama => {}
    }
    Ok(built
        .model(request.model)
        .preamble(request.instructions)
        .tools(tools)
        .provider_tools(hosted.collect()))
}

/// The wire's own spec for a hosted tool, when the wire offers one. Chat
/// offers none: a stray call reaches the router, which refuses it with a code.
fn hosted(wire: Wire, tool: Hosted) -> Option<ProviderToolDefinition> {
    match (tool, wire) {
        (Hosted::WebSearch, Wire::Messages) => Some(
            ProviderToolDefinition::new(WEB_SEARCH_MESSAGES)
                .with_config(FIELD_NAME, serde_json::Value::from(tool.name())),
        ),
        (Hosted::WebSearch, Wire::Responses) => Some(ProviderToolDefinition::new(tool.name())),
        (Hosted::WebSearch, Wire::Chat | Wire::Gemini | Wire::Ollama) => None,
    }
}

/// The conversation as it is mapped: every call's tool by its id, the id
/// borrowed from the conversation and the name already checked, so a result
/// names it without a second check.
#[derive(Debug, Default)]
struct Conversation<'a> {
    tools: HashMap<&'a str, (ToolName, CallId)>,
}

impl<'a> Conversation<'a> {
    /// `messages` as rig's history: consecutive user content, a turn's tool
    /// results and the user message after them, kept as one user turn.
    fn map(mut self, messages: &'a [Message]) -> Result<Vec<RigMessage>> {
        let mut history: Vec<RigMessage> = Vec::new();
        for message in messages {
            match message {
                Message::User(text) => user(&mut history, UserContent::text(text.clone())),
                Message::Assistant {
                    text,
                    calls,
                    replay,
                } => {
                    let content = self.assistant(text, calls, &replay.content)?;
                    if !content.is_empty() {
                        let turn =
                            AssistantMessage::new(content).with_origin(replay.origin.clone());
                        history.push(RigMessage::Assistant(turn));
                    }
                }
                Message::ToolResult {
                    call_id,
                    output,
                    image,
                } => {
                    let result = self.result(call_id, output, image.as_ref())?;
                    user(&mut history, result);
                }
            }
        }
        Ok(history)
    }

    /// An assistant turn: the reasoning handed back first, then the text, then
    /// each call under the id its result echoes.
    fn assistant(
        &mut self,
        text: &str,
        calls: &'a [Call],
        replay: &[AssistantContent],
    ) -> Result<Vec<AssistantContent>> {
        let built_calls = calls
            .iter()
            .map(|call| self.call(call, replay))
            .collect::<Result<Vec<_>>>()?;
        if unchanged(replay, text, &built_calls) {
            return Ok(replay.to_vec());
        }
        let mut content: Vec<_> = replay
            .iter()
            .filter(|item| matches!(item, AssistantContent::Reasoning(_)))
            .cloned()
            .collect();
        if !text.is_empty() {
            content.push(AssistantContent::text(text.to_owned()));
        }
        content.extend(built_calls);
        Ok(content)
    }

    /// Keeps rig's call identity so its native fingerprint and result still agree.
    fn call(&mut self, call: &'a Call, replay: &[AssistantContent]) -> Result<AssistantContent> {
        let name =
            ToolName::new(call.name.clone()).map_err(|source| raise::unnamed(&call.id, source))?;
        let original = replay.iter().find_map(|item| match item {
            AssistantContent::ToolCall(original) if original.id.wire() == call.id => Some(original),
            _ => None,
        });
        let id = original.map_or_else(
            || CallId::from_wire(call.id.clone()),
            |original| original.id.clone(),
        );
        let function = ToolFunction::new(name.clone(), call.arguments.clone());
        let mut built = ToolCall::new(id.clone(), function);
        built.native = original.and_then(|original| original.native.clone());
        self.tools.insert(&call.id, (name, id));
        Ok(AssistantContent::ToolCall(built))
    }

    /// One call's result, under the tool its call named: its text, then the
    /// image it read, when it read one.
    fn result(
        &self,
        call_id: &str,
        output: &str,
        image: Option<&ImageInput>,
    ) -> Result<UserContent> {
        let (name, id) = self
            .tools
            .get(call_id)
            .cloned()
            .ok_or_else(|| raise::unsendable(call_id))?;
        let mut content = vec![ToolResultContent::text(output.to_owned())];
        content.extend(image.map(image_input::content));
        Ok(UserContent::tool_result(id, name, content))
    }
}

/// Native parts keep their order and signatures while their public fields agree.
fn unchanged(replay: &[AssistantContent], text: &str, calls: &[AssistantContent]) -> bool {
    let answer: String = replay
        .iter()
        .filter_map(|item| match item {
            AssistantContent::Text(part) => Some(part.text.as_str()),
            _ => None,
        })
        .collect();
    answer == text
        && replay
            .iter()
            .filter(|item| matches!(item, AssistantContent::ToolCall(_)))
            .eq(calls.iter())
}

/// Adds `content` to the user turn the history ends on, or opens one.
fn user(history: &mut Vec<RigMessage>, content: UserContent) {
    if let Some(RigMessage::User { content: open }) = history.last_mut() {
        open.push(content);
    } else {
        history.push(RigMessage::User {
            content: vec![content],
        });
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod image_tests;
