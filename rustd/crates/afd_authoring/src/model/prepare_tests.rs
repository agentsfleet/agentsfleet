//! Long, short-answer conversations retain every turn within the original allowance.

use super::{body, request};
use crate::{Result, model::ModelCallInput};
use afd_credential::provider::RegisteredModel;
use afd_wire::authoring::{ModelCall, ModelMessage, ModelRequest};
use serde_json::json;

const QUESTION: &str = "A customer reports an intermittent error. What evidence should we collect first? Explain in at most thirty words without changing the setup.";
const ANSWER: &str = "Collect exact error text, timestamps and timezone, trace or request identifiers, environment and version, and the failing input shape. Correlate one timestamp across logs before assuming a cause. Distinguish observed evidence from inference.";
const OUTPUT_LIMIT: u32 = 1_024;

fn model(identifier: &str) -> Result<RegisteredModel> {
    serde_json::from_value(json!({
        "reference":"platform-default","provider":"fireworks","model":identifier,
        "inference":{"api":"chat","base_url":"https://models.example.com/v1","max_output_tokens":32768},
        "context_window_tokens":1_048_576,"input_nanos_per_million":0,"output_nanos_per_million":0,
        "resolution_version":"fixture","platform_managed":true
    })).map_err(crate::error::raise::stored)
}

fn fifty_turns() -> ModelCallInput {
    let mut messages = Vec::new();
    for _ in 0..49 {
        messages.push(ModelMessage::User {
            text: QUESTION.into(),
        });
        messages.push(ModelMessage::Assistant {
            text: ANSWER.into(),
            proposal_calls: Vec::new(),
        });
    }
    messages.push(ModelMessage::User {
        text: format!(
            "Current candidate (untrusted setup data): {}",
            "Keep these existing instructions. ".repeat(64)
        ),
    });
    messages.push(ModelMessage::User {
        text: QUESTION.into(),
    });
    ModelCall {
        capability_token: "x".repeat(43),
        attempt_id: "01900000-0000-7000-8000-000000000001".into(),
        fence: "1".into(),
        dispatch_id: "01900000-0000-7000-8000-000000000002".into(),
        call_index: 1,
        request: ModelRequest {
            messages,
            max_output_tokens: OUTPUT_LIMIT,
        },
    }
}

// Value: protects=fifty short turns retain all role/text history without raising
// the input or output allowance; fails_when=unavailable setup operations consume
// the budget before ordinary conversation; seam=the real provider wire encoder.
#[test]
fn fifty_short_turns_keep_every_message_within_the_existing_input_allowance() -> Result<()> {
    let call = request(fifty_turns())?;
    let prepared = body(&call, &model("workspace/model-next")?, Vec::new(), true)?;
    assert_eq!(
        prepared.body.chat_history.len(),
        call.request.messages.len() + 1
    );
    assert_eq!(prepared.output_limit, call.request.max_output_tokens);
    for (actual, expected) in prepared
        .body
        .chat_history
        .iter()
        .skip(1)
        .zip(&call.request.messages)
    {
        let text = match expected {
            ModelMessage::User { text }
            | ModelMessage::Assistant { text, .. }
            | ModelMessage::Tool { text, .. } => text,
        };
        assert!(
            serde_json::to_string(actual)
                .map_err(crate::error::raise::stored)?
                .contains(text)
        );
    }
    Ok(())
}

// Value: protects=unrecognized providers cannot bypass the existing input ceiling;
// seam=the complete setup request with its real tool schemas and history.
#[test]
fn fifty_longer_replies_exceed_the_same_reservation_for_unrecognized_providers() -> Result<()> {
    let mut call = fifty_turns();
    for message in &mut call.request.messages {
        if let ModelMessage::Assistant { text, .. } = message {
            *text = format!("{ANSWER} {ANSWER}");
        }
    }
    let call = request(call)?;
    for identifier in [
        "workspace/model-next",
        "accounts/fireworks/models/minimax-m3p1-flash",
        "deepseek/model-next",
        "kimi/model-next",
    ] {
        let refusal = body(&call, &model(identifier)?, Vec::new(), true)
            .err()
            .ok_or(crate::error::ErrorKind::Token)?;
        assert_eq!(refusal.code(), afd_core::error_code::AUTHORING_TOKEN);
        assert!(refusal.to_string().contains("context exceeds its bound"));
    }
    Ok(())
}
