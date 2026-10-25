//! Count the selected wire's complete input, including replay and inline media.
use super::*;
use rig_core::{
    completion::{ToolAnnotations, ToolDefinition},
    message::{
        AssistantContent, AssistantMessage, CallId, DocumentMediaType, ImageMediaType, ToolName,
        ToolResultContent, UserContent,
    },
};
use serde_json::{Value, json};

fn prepared() -> CompletionRequest {
    let name = ToolName::new("inspect_file").unwrap();
    let mut request = body();
    request.tools.push(
        ToolDefinition {
            name: name.clone(),
            description: "private schema description".into(),
            parameters: json!({"type":"object","properties":{"path":{"type":"string"}},
            "required":["path"]}),
            annotations: ToolAnnotations::default(),
        }
        .into(),
    );
    request
        .chat_history
        .push(Arc::new(Message::Assistant(AssistantMessage::new(vec![
            AssistantContent::reasoning("private replay thought"),
            AssistantContent::tool_call(
                "call-inspect",
                name.clone(),
                json!({"path":"private-path"}),
            ),
        ]))));
    request.chat_history.push(Arc::new(Message::User {
        content: vec![
            UserContent::tool_result(
                CallId::from_wire("call-inspect"),
                name,
                vec![
                    ToolResultContent::text("private tool result"),
                    ToolResultContent::image_base64("iVBORw==", Some(ImageMediaType::PNG), None),
                ],
            ),
            UserContent::document_base64("JVBERi0=", Some(DocumentMediaType::PDF)),
        ],
    }));
    request
}

fn inputs(api: InferenceApi) -> &'static [&'static str] {
    match api {
        InferenceApi::Responses => &["input", "instructions", "tools"],
        InferenceApi::Messages => &["messages", "system", "tools"],
        InferenceApi::Gemini => &["contents", "systemInstruction", "tools"],
        _ => &[],
    }
}

#[tokio::test]
async fn native_payload_preserves_tools_results_inline_media_and_selected_replay() {
    for api in [
        InferenceApi::Responses,
        InferenceApi::Messages,
        InferenceApi::Gemini,
    ] {
        let settings = native(api);
        let route = Configured::new(&settings, 512).unwrap();
        let request = prepared();
        let encoded = route.encode_stream("model-next", request.clone()).unwrap();
        let wire = crate::json_request(encoded.request).unwrap();
        let generated: Value = serde_json::from_slice(wire.body()).unwrap();
        let reply = json!({"input_tokens":123,"totalTokens":123}).to_string();
        let physical = physical(200, Some(&reply), false);
        let mut count = route
            .confirm_input(
                "model-next",
                "private-credential",
                &request,
                &HttpTransport::new(Arc::<Physical>::clone(&physical)),
                &TurnControl::default(),
            )
            .await
            .unwrap();
        assert_eq!(count.method(), "provider_count");
        assert_payload(api, &generated, &physical);
        let mut changed = request;
        changed.tools.clear();
        let error = route
            .bound_counted_output("model-next", changed, &mut count)
            .unwrap_err();
        assert!(matches!(error.kind(), crate::error::ErrorKind::CountStale));
        assert_eq!(physical.requests.lock().unwrap().len(), 1);
    }
}

fn assert_payload(api: InferenceApi, generated: &Value, physical: &Physical) {
    let requests = physical.requests.lock().unwrap();
    let counted: Value = serde_json::from_slice(requests.first().unwrap().body()).unwrap();
    let counted = if api == InferenceApi::Gemini {
        counted.get("generateContentRequest").unwrap()
    } else {
        &counted
    };
    for field in inputs(api) {
        assert!(
            generated.get(*field).is_some(),
            "selected wire carries {field}"
        );
        assert_eq!(
            counted.get(*field),
            generated.get(*field),
            "{api:?} {field}"
        );
    }
    let encoded = counted.to_string();
    for content in [
        "private schema description",
        "private tool result",
        "private replay thought",
        "private-path",
        "iVBORw==",
        "JVBERi0=",
    ] {
        assert!(encoded.contains(content), "{api:?} omits {content}");
    }
}

#[test]
fn changing_media_invalidates_a_previous_measurement() {
    for api in [
        InferenceApi::Responses,
        InferenceApi::Messages,
        InferenceApi::Gemini,
    ] {
        let settings = native(api);
        let route = Configured::new(&settings, LARGE_WINDOW).unwrap();
        let request = prepared();
        let mut count = route.input_count("model-next", &request).unwrap();
        let mut changed = request;
        changed.chat_history.push(Arc::new(Message::User {
            content: vec![UserContent::image_url(
                "https://images.example/private.png",
                Some(ImageMediaType::PNG),
                None,
            )],
        }));
        let error = route
            .bound_counted_output("model-next", changed, &mut count)
            .unwrap_err();
        assert!(matches!(error.kind(), crate::error::ErrorKind::CountStale));
    }
}

#[test]
fn counted_stream_uses_the_requests_explicit_model_override() {
    for api in [
        InferenceApi::Chat,
        InferenceApi::Responses,
        InferenceApi::Messages,
        InferenceApi::Gemini,
        InferenceApi::Ollama,
    ] {
        let settings = settings(api);
        let route = Configured::new(&settings, LARGE_WINDOW).unwrap();
        let request = body().model("request-specific-tag");
        let count = route.input_count("request-specific-tag", &request).unwrap();
        let physical = physical(200, Some("{}"), false);
        let model = route.model(
            "saved-default-tag",
            "private-credential",
            HttpTransport::new(Arc::<Physical>::clone(&physical)),
        );
        let turn = crate::stream::StreamTurn::open_counted(
            &route,
            &model,
            request,
            count,
            TurnControl::default(),
        )
        .unwrap();
        drop(turn);
        assert!(physical.requests.lock().unwrap().is_empty());
    }
}
