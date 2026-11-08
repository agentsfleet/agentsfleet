//! Long, short-answer conversations retain every turn within the original allowance.

use super::{body, request};
use crate::{Result, model::ModelCallInput};
use af_wire::authoring::{ModelCall, ModelMessage, ModelRequest};
use afd_credential::provider::RegisteredModel;
use rig_core::message::Message;
use serde_json::json;

const QUESTION: &str = "A customer reports an intermittent error. What evidence should we collect first? Explain in at most thirty words without changing the setup.";
const ANSWER: &str = "Collect exact error text, timestamps and timezone, trace or request identifiers, environment and version, and the failing input shape. Correlate one timestamp across logs before assuming a cause. Distinguish observed evidence from inference.";
const OUTPUT_LIMIT: u32 = 1_024;
/// A log identifier as a reply quotes one: 43 bytes, 23 `o200k_base` tokens.
const TRACE: &str = "trace=01a18bc1-d47c-7003-98b2-fba3847cc642 ";

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
            supports_compaction: true,
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
    assert!(prepared.body.system_instructions().is_none());
    assert_eq!(prepared.body.chat_history.len(), 100);
    assert_eq!(
        prepared.body.chat_history.len(),
        call.request.messages.len()
    );
    assert_eq!(prepared.output_limit, call.request.max_output_tokens);
    for (actual, expected) in prepared
        .body
        .chat_history
        .iter()
        .zip(&call.request.messages)
    {
        let (text, role_matches) = match expected {
            ModelMessage::User { text } | ModelMessage::Tool { text, .. } => {
                (text, matches!(actual.as_ref(), Message::User { .. }))
            }
            ModelMessage::Assistant { text, .. } => {
                (text, matches!(actual.as_ref(), Message::Assistant(_)))
            }
        };
        assert!(role_matches);
        assert!(
            serde_json::to_string(actual)
                .map_err(crate::error::raise::stored)?
                .contains(text)
        );
    }
    Ok(())
}

// Value: protects=the selected model window replaces the artificial input ceiling;
// seam=the complete setup request with its real tool schemas and history.
#[test]
fn fifty_longer_replies_use_the_selected_model_window_beyond_thirty_two_k() -> Result<()> {
    let mut call = fifty_turns();
    for message in &mut call.request.messages {
        if let ModelMessage::Assistant { text, .. } = message {
            // Sized in estimator tokens under the 131,072-byte request cap.
            // Prose packs about five bytes per `o200k_base` token, so no prose
            // conversation that fits the cap passes 32,768. Trace identifiers
            // pack under two: 49 replies of 24 come to about 36,500 estimated
            // tokens in about 62,000 bytes.
            *text = format!("{ANSWER} {}", TRACE.repeat(24));
        }
    }
    let call = request(call)?;
    for identifier in [
        "workspace/model-next",
        "accounts/fireworks/models/minimax-m3p1-flash",
        "deepseek/model-next",
        "kimi/model-next",
    ] {
        let prepared = body(&call, &model(identifier)?, Vec::new(), true)?;
        assert!(prepared.input_tokens > 32_768);
        assert_eq!(prepared.output_limit, OUTPUT_LIMIT);
        assert!(prepared.body.system_instructions().is_none());
        assert_eq!(prepared.body.chat_history.len(), 100);
        assert_eq!(
            prepared.body.chat_history.len(),
            call.request.messages.len()
        );
    }
    Ok(())
}

// Value: mandatory history and tool schemas cannot consume the selected physical window.
#[test]
fn an_irreducible_request_refuses_before_any_output_is_admitted() -> Result<()> {
    let call = request(fifty_turns())?;
    let mut selected = model("workspace/model-next")?;
    selected.context_window_tokens = 1_024;
    selected
        .inference
        .as_mut()
        .ok_or(crate::error::ErrorKind::Model)?
        .max_output_tokens = OUTPUT_LIMIT;
    let error = body(&call, &selected, Vec::new(), true)
        .err()
        .ok_or(crate::error::ErrorKind::Provider)?;
    assert_eq!(error.code(), af_core::error_code::AUTHORING_TOKEN);
    Ok(())
}
