//! Unsupported count inputs refuse locally, without a paid generation fallback.
use super::*;
use rig_core::completion::{ToolAnnotations, ToolDefinition};
use rig_core::message::{
    AssistantContent, AssistantMessage, CallId, Document, DocumentMediaType, DocumentSourceKind,
    Image, ImageMediaType, ToolName, ToolResultContent, UserContent,
};
use serde_json::json;

#[tokio::test]
async fn anthropic_server_tools_and_connectors_refuse_before_network() {
    for params in [
        json!({"tools":[{"type":"web_search_20250305","name":"web_search"}]}),
        json!({"tools":[{"type":"web_fetch_20250910","name":"web_fetch"}]}),
        json!({"tools":[{"type":"code_execution_20250825","name":"code_execution"}]}),
        json!({"tools":[{"type":"tool_search_tool_regex_20251119","name":"tool_search_tool_regex"}]}),
        json!({"tools":[{"type":"future_server_tool","name":"private-tool"}]}),
        json!({"mcp_servers":[{"type":"url","url":"https://private.example"}]}),
    ] {
        assert_refused(body().additional_params(params)).await;
    }
}

#[tokio::test]
async fn anthropic_external_media_refuses_before_network() {
    for (index, content) in [
        UserContent::image_url(
            "https://private.example/image",
            Some(ImageMediaType::PNG),
            None,
        ),
        UserContent::document_url(
            "https://private.example/document",
            Some(DocumentMediaType::PDF),
        ),
        UserContent::Image(Image {
            data: DocumentSourceKind::file_id("private-image-file"),
            media_type: Some(ImageMediaType::PNG),
            ..Default::default()
        }),
        UserContent::Document(Document {
            data: DocumentSourceKind::file_id("private-document-file").into(),
            media_type: Some(DocumentMediaType::PDF),
            ..Default::default()
        }),
        UserContent::tool_result(
            CallId::from_wire("private-tool-call"),
            ToolName::new("inspect").unwrap(),
            vec![ToolResultContent::image_url(
                "https://private.example/nested-image",
                Some(ImageMediaType::PNG),
                None,
            )],
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let mut request = body();
        if matches!(&content, UserContent::ToolResult(_)) {
            request.tools.push(
                ToolDefinition {
                    name: ToolName::new("inspect").unwrap(),
                    description: "inspect a file".into(),
                    parameters: json!({"type":"object","properties":{}}),
                    annotations: ToolAnnotations::default(),
                }
                .into(),
            );
            request
                .chat_history
                .push(Arc::new(Message::Assistant(AssistantMessage::new(vec![
                    AssistantContent::tool_call(
                        "private-tool-call",
                        ToolName::new("inspect").unwrap(),
                        json!({}),
                    ),
                ]))));
        }
        request.chat_history.push(Arc::new(Message::User {
            content: vec![content],
        }));
        assert_refused_case(request, &format!("unsupported media case {index}")).await;
    }
}

#[tokio::test]
async fn anthropic_client_tools_and_advisor_keep_native_counting() {
    for kind in [
        "custom",
        "bash_20250124",
        "text_editor_20250728",
        "computer_20250124",
        "advisor_20260301",
    ] {
        let request = body().additional_params(json!({"tools":[{
            "type":kind,"name":"inspect","input_schema":{"type":"object","properties":{}}
        }]}));
        let settings = native(InferenceApi::Messages);
        let route = Configured::new(&settings, 512).unwrap();
        let physical = physical(200, Some(r#"{"input_tokens":123}"#), false);
        let count = route
            .confirm_input(
                "model-next",
                "private-credential",
                &request,
                &HttpTransport::new(Arc::<Physical>::clone(&physical)),
                &TurnControl::default(),
            )
            .await
            .unwrap();
        assert_eq!(count.tokens(), 123);
        assert_eq!(physical.requests.lock().unwrap().len(), 1);
    }
}

async fn assert_refused(request: CompletionRequest) {
    assert_refused_case(request, "unsupported count input").await;
}

async fn assert_refused_case(request: CompletionRequest, case: &str) {
    let settings = native(InferenceApi::Messages);
    let route = Configured::new(&settings, 512).unwrap();
    let physical = physical(200, Some(r#"{"input_tokens":123}"#), false);
    let error = route
        .confirm_input(
            "model-next",
            "private-credential",
            &request,
            &HttpTransport::new(Arc::<Physical>::clone(&physical)),
            &TurnControl::default(),
        )
        .await
        .unwrap_err();
    assert_eq!(
        error.code(),
        afd_core::error_code::INVALID_REQUEST,
        "{case}"
    );
    assert!(
        error
            .to_string()
            .contains("does not support this prepared input")
    );
    assert!(!format!("{error:?}").contains("private"));
    assert!(physical.requests.lock().unwrap().is_empty());
}
