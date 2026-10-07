#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use bytes::Bytes;
use rig_core::completion::CompletionRequest;
use rig_core::message::{
    AssistantContent, DocumentSourceKind, ImageMediaType, Message as RigMessage, ToolName,
    ToolResultContent, UserContent,
};
use serde_json::{Value, json};

use super::request;
use crate::error::raise;
use crate::image_input::{ImageInput, ImageKind};
use crate::provider::{Call, Hosted, Message, Replay, Request, ToolSpec};
use crate::registry::Wire;

/// The model every turn here names.
const MODEL: &str = "model-1";
/// The conversation cache key every request here carries.
const CACHE_KEY: &str = "01924f4e-0000-7000-8000-00000000fee7";
/// The system prompt every turn here carries.
const INSTRUCTIONS: &str = "Read the run.";
/// The question the conversation opens with.
const QUESTION: &str = "why did the build fail?";
/// What the model said before its call.
const PREAMBLE: &str = "Checking the log.";
/// The call the conversation's assistant turn made.
const CALL_ID: &str = "call-1";
/// The tool it called.
const TOOL: &str = "update_plan";
/// What that call returned.
const OUTPUT: &str = "4";
/// What the loop said after the result.
const FOLLOW_UP: &str = "Answer now.";
/// The reasoning a provider asked to have back.
const THOUGHT: &str = "the log names a missing file";

/// An assistant turn that said `text`, made `calls` and reasoned `replay`.
fn said(text: &str, calls: Vec<Call>, replay: Replay) -> Message {
    Message::Assistant {
        text: text.to_owned(),
        calls,
        replay,
    }
}

/// The plan tool, called under [`CALL_ID`].
fn plan_call() -> Call {
    Call {
        id: CALL_ID.to_owned(),
        name: TOOL.to_owned(),
        arguments: json!({"expression": "2+2"}),
    }
}

/// A conversation with every message kind: a question, a turn that said
/// something and called a tool, the call's result, and a user message after
/// it.
fn conversation() -> Vec<Message> {
    vec![
        Message::User(QUESTION.to_owned()),
        said(PREAMBLE, vec![plan_call()], Replay::default()),
        Message::ToolResult {
            call_id: CALL_ID.to_owned(),
            output: OUTPUT.to_owned(),
            image: None,
        },
        Message::User(FOLLOW_UP.to_owned()),
    ]
}

/// `messages` over `wire`, offering `tools` and `hosted`, as rig sends it.
fn built(
    wire: Wire,
    messages: &[Message],
    tools: &[ToolSpec<'_>],
    hosted: &[Hosted],
) -> CompletionRequest {
    let turn = Request {
        model: MODEL,
        instructions: INSTRUCTIONS,
        messages,
        tools,
        hosted,
        cache_key: CACHE_KEY,
    };
    request(wire, &turn).unwrap()
}

/// The text `content` holds, when it is text.
fn user_text(content: &UserContent) -> Option<&str> {
    match content {
        UserContent::Text(text) => Some(&text.text),
        _ => None,
    }
}

#[test]
fn should_lead_with_the_instructions_and_answer_each_call_under_its_tool() {
    let messages = conversation();

    let sent = built(Wire::Chat, &messages, &[], &[]);

    assert_eq!(sent.model.as_deref(), Some(MODEL));
    assert_eq!(sent.system_instructions(), Some(INSTRUCTIONS));
    let [
        _,
        RigMessage::User { content: asked },
        RigMessage::Assistant { content: turn, .. },
        RigMessage::User { content: answered },
    ] = sent.chat_history.as_slice()
    else {
        panic!("system, question, turn, answer: {:?}", sent.chat_history);
    };
    assert_eq!(
        asked.iter().filter_map(user_text).collect::<Vec<_>>(),
        [QUESTION]
    );
    let [
        AssistantContent::Text(text),
        AssistantContent::ToolCall(call),
    ] = turn.as_slice()
    else {
        panic!("its text, then its call: {turn:?}");
    };
    assert_eq!(
        (text.text.as_str(), call.id.wire().as_ref()),
        (PREAMBLE, CALL_ID)
    );
    let [UserContent::ToolResult(result), UserContent::Text(after)] = answered.as_slice() else {
        panic!("the result and the message after it, as one user turn: {answered:?}");
    };
    assert_eq!(result.call.wire(), CALL_ID);
    assert_eq!(result.name.as_str(), TOOL, "the tool its call named");
    assert_eq!(after.text, FOLLOW_UP);
}

#[test]
fn should_hand_the_replay_back_ahead_of_the_turns_text() {
    let replay = Replay(vec![AssistantContent::reasoning("anthropic", THOUGHT)]);
    let messages = [
        Message::User(QUESTION.to_owned()),
        said(PREAMBLE, Vec::new(), replay),
    ];

    let sent = built(Wire::Messages, &messages, &[], &[]);

    let Some(RigMessage::Assistant { content, .. }) = sent.chat_history.last() else {
        panic!("the turn ends the history: {:?}", sent.chat_history);
    };
    assert!(
        matches!(
            content.as_slice(),
            [AssistantContent::Reasoning(_), AssistantContent::Text(_)]
        ),
        "{content:?}"
    );
}

#[test]
fn should_drop_an_assistant_turn_with_nothing_in_it() {
    let messages = [
        Message::User(QUESTION.to_owned()),
        said("", Vec::new(), Replay::default()),
        Message::User(FOLLOW_UP.to_owned()),
    ];

    let sent = built(Wire::Chat, &messages, &[], &[]);

    assert!(
        !sent
            .chat_history
            .iter()
            .any(|message| matches!(message, RigMessage::Assistant { .. })),
        "{:?}",
        sent.chat_history
    );
}

#[test]
fn should_refuse_a_result_that_answers_no_call() {
    let messages = [
        Message::User(QUESTION.to_owned()),
        Message::ToolResult {
            call_id: "call-nobody-made".to_owned(),
            output: OUTPUT.to_owned(),
            image: None,
        },
    ];
    let turn = Request {
        model: MODEL,
        instructions: INSTRUCTIONS,
        messages: &messages,
        tools: &[],
        hosted: &[],
        cache_key: CACHE_KEY,
    };

    let refused = request(Wire::Chat, &turn).unwrap_err();

    assert!(
        refused.detail().contains("call-nobody-made"),
        "{}",
        refused.detail()
    );
    assert_eq!(refused.failure_class(), None);
}

#[test]
fn should_offer_each_function_as_its_spec_describes_it() {
    let parameters = json!({"type": "object"});
    let tools = [ToolSpec {
        name: TOOL,
        description: TOOL,
        parameters: &parameters,
    }];

    let sent = built(Wire::Responses, &conversation(), &tools, &[]);

    let [offered] = sent.tools.as_slice() else {
        panic!("one function: {:?}", sent.tools);
    };
    assert_eq!(
        (offered.name.as_str(), &offered.parameters),
        (TOOL, &parameters)
    );
}

#[test]
fn should_offer_web_search_as_each_wire_spells_it_and_chat_not_at_all() {
    let hosted = |wire| {
        let sent = built(wire, &conversation(), &[], &[Hosted::WebSearch]);
        sent.additional_params.map(|params| params["tools"].clone())
    };

    let messages = hosted(Wire::Messages).unwrap();
    let responses = hosted(Wire::Responses).unwrap();

    assert_eq!(
        messages,
        json!([{"type": "web_search_20250305", "name": Hosted::WebSearch.name()}])
    );
    assert_eq!(responses, json!([{"type": Hosted::WebSearch.name()}]));
    assert_eq!(hosted(Wire::Chat), None::<Value>);
}

#[test]
fn should_refuse_a_call_that_names_no_tool() {
    let nameless = Call {
        name: String::new(),
        ..plan_call()
    };
    let messages = [
        Message::User(QUESTION.to_owned()),
        said(PREAMBLE, vec![nameless], Replay::default()),
    ];
    let turn = Request {
        model: MODEL,
        instructions: INSTRUCTIONS,
        messages: &messages,
        tools: &[],
        hosted: &[],
        cache_key: CACHE_KEY,
    };

    let refused = request(Wire::Chat, &turn).unwrap_err();

    let empty = ToolName::new(String::new()).unwrap_err();
    assert_eq!(refused.detail(), raise::unnamed(CALL_ID, empty).detail());
    assert_eq!(refused.failure_class(), None);
}

/// An image a call read goes back inside its result, after the text, as
/// base64 under its media type: the shape rig turns into a Messages `image`
/// block and a Responses `input_image`.
#[test]
fn should_carry_a_tool_results_image_as_base64_content_after_its_text() {
    let mut messages = conversation();
    messages[2] = Message::ToolResult {
        call_id: CALL_ID.to_owned(),
        output: OUTPUT.to_owned(),
        image: Some(ImageInput {
            kind: ImageKind::Png,
            data: Bytes::from_static(b"\x89PNG"),
        }),
    };

    let sent = built(Wire::Messages, &messages, &[], &[]);

    let RigMessage::User { content: answered } = &sent.chat_history[3] else {
        panic!("the result is a user turn: {:?}", sent.chat_history);
    };
    let UserContent::ToolResult(result) = &answered[0] else {
        panic!("the result leads the turn: {answered:?}");
    };
    let parts: Vec<&ToolResultContent> = result.content.iter().collect();
    let [
        ToolResultContent::Text(text),
        ToolResultContent::Image(image),
    ] = parts.as_slice()
    else {
        panic!("the text, then the image: {parts:?}");
    };
    assert_eq!(text.text, OUTPUT);
    // pin test: literal is the contract
    assert_eq!(
        image.data,
        DocumentSourceKind::Base64("iVBORw==".to_owned())
    );
    assert_eq!(image.media_type, Some(ImageMediaType::PNG));
    assert_eq!(image.detail, None, "each wire applies its own default");
}

/// Each kind the seam names goes out under its own media type.
#[test]
fn should_name_each_image_kinds_media_type() {
    for (kind, media_type) in [
        (ImageKind::Png, ImageMediaType::PNG),
        (ImageKind::Jpeg, ImageMediaType::JPEG),
        (ImageKind::Gif, ImageMediaType::GIF),
        (ImageKind::Webp, ImageMediaType::WEBP),
    ] {
        let mut messages = conversation();
        messages[2] = Message::ToolResult {
            call_id: CALL_ID.to_owned(),
            output: OUTPUT.to_owned(),
            image: Some(ImageInput {
                kind,
                data: Bytes::from_static(b"\x00"),
            }),
        };

        let sent = built(Wire::Responses, &messages, &[], &[]);

        let RigMessage::User { content: answered } = &sent.chat_history[3] else {
            panic!("the result is a user turn: {:?}", sent.chat_history);
        };
        let UserContent::ToolResult(result) = &answered[0] else {
            panic!("the result leads the turn: {answered:?}");
        };
        let Some(ToolResultContent::Image(image)) = result.content.get(1) else {
            panic!("{kind:?}: the image follows the text: {:?}", result.content);
        };
        assert_eq!(image.media_type, Some(media_type), "{kind:?}");
    }
}
