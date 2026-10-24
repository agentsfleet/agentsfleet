//! Native routes, request bounds and the typed failures consumed by both adapters.

use super::*;

#[test]
fn known_tokenizers_count_tokens_and_unknown_models_reserve_utf8_bytes() {
    assert_eq!(crate::input_tokens("gpt-4o", "hello world"), 2);
    assert_eq!(crate::input_tokens("unknown-tokenizer", "hello world"), 11);
    assert_eq!(crate::input_tokens("unknown-tokenizer", "🌿"), 4);
}

// Value: protects=changing providers never bypasses the shared input reservation;
// seam=the public counter used by both setup and runtime admission.
#[test]
fn providers_without_a_library_tokenizer_share_the_utf8_reservation() {
    for model in [
        "accounts/fireworks/models/minimax-m3p1-flash",
        "minimax/minimax-m3.1-flash:free",
        "MiniMaxAI/MiniMax-M3.1-Flash",
        "MiniMax-M3.1-Flash",
        "deepseek/model-next",
        "kimi/model-next",
    ] {
        for text in ["", "hello world", "🌿", "Cafe\u{301}", "<tool_call>"] {
            assert_eq!(crate::input_tokens(model, text), text.len(), "{model}");
        }
    }
}

#[test]
fn native_routes_advertise_tool_support_through_the_sdk() {
    for api in [InferenceApi::Gemini, InferenceApi::Ollama] {
        let value = settings(api);
        let route = Configured::new(&value, 128_000).unwrap();
        assert!(route.supports_tools("configured-model"), "{api:?}");
    }
}

#[test]
fn native_request_encoders_keep_the_explicit_output_allowance() {
    for (api, field) in [
        (InferenceApi::Messages, "/max_tokens"),
        (InferenceApi::Gemini, "/generationConfig/maxOutputTokens"),
        (InferenceApi::Ollama, "/options/num_predict"),
    ] {
        let value = settings(api);
        let route = Configured::new(&value, 128_000).unwrap();
        for streaming in [false, true] {
            let mut request = CompletionRequest::new(Message::user("hello"));
            request.max_tokens = Some(17);
            let encoded = if streaming {
                route.encode_stream("configured-model", request)
            } else {
                route.encode("configured-model", request)
            }
            .unwrap();
            let request = json_request(encoded.request).unwrap();
            assert_eq!(request.uri().host(), Some("inference.example"));
            let body: serde_json::Value = serde_json::from_slice(request.body()).unwrap();
            assert_eq!(body.pointer(field), Some(&serde_json::json!(17)), "{api:?}");
        }
    }
}

#[test]
fn context_admission_reserves_complete_input_before_output() {
    let value = settings(InferenceApi::Chat);
    let route = Configured::new(&value, 128_000).unwrap();
    let request = route
        .bound_output(
            "unknown-tokenizer",
            CompletionRequest::new(Message::user("hello")),
        )
        .unwrap();
    assert_eq!(request.max_tokens, Some(37));
    let route = Configured::new(&value, 100).unwrap();
    let request = CompletionRequest::new(Message::user("large prompt ".repeat(100)));
    let refusal = route
        .bound_output("unknown-tokenizer", request)
        .unwrap_err();
    assert_eq!(refusal.code(), afd_core::error_code::INVALID_REQUEST);
    assert!(!refusal.is_oversize());
    assert!(refusal.to_string().contains("context"));
}

// Value: protects=the byte reservation still clamps output and rejects exhausted
// context after switching providers; seam=the real streaming request encoder.
#[test]
fn unrecognized_providers_reserve_input_before_the_remaining_output() {
    let value = settings(InferenceApi::Chat);
    for model in [
        "accounts/fireworks/models/minimax-m3p1-flash",
        "deepseek/model-next",
        "kimi/model-next",
    ] {
        let request = CompletionRequest::new(Message::user("🌿 hello"));
        let route = Configured::new(&value, 128_000).unwrap();
        let encoded = route.encode_stream(model, request.clone()).unwrap();
        let body = json_request(encoded.request).unwrap();
        let input = u32::try_from(body.body().len() + 32).unwrap();
        let route = Configured::new(&value, input + 7).unwrap();
        let bounded = route.bound_output(model, request.clone()).unwrap();
        assert_eq!(bounded.max_tokens, Some(7));
        let route = Configured::new(&value, input).unwrap();
        let refusal = route.bound_output(model, request).unwrap_err();
        assert_eq!(refusal.code(), afd_core::error_code::INVALID_REQUEST);
        assert!(refusal.to_string().contains("context"));
    }
}

#[test]
fn an_owned_route_keeps_validated_routing_after_its_input_is_dropped() {
    let route = {
        let value = settings(InferenceApi::Messages);
        Configured::new(&value, 128_000).unwrap().owned()
    };
    assert!(route.accepts_endpoint(Some("https://inference.example/custom/v1")));
    assert!(!route.accepts_endpoint(Some("https://unrelated.example/v1")));
    assert!(route.supports_tools("configured-model"));
    let encoded = route
        .encode(
            "configured-model",
            CompletionRequest::new(Message::user("hello")),
        )
        .unwrap();
    let request = json_request(encoded.request).unwrap();
    assert_eq!(request.uri().path(), "/custom/v1/messages");
    assert_eq!(route.into_host().as_ref(), "inference.example");
}

#[test]
fn provider_failures_keep_their_registry_classification_and_size_identity() {
    for kind in [
        ErrorKind::ReplyTooLarge,
        ErrorKind::StreamDeadline,
        ErrorKind::StreamCancelled,
    ] {
        let size_failure = matches!(kind, ErrorKind::ReplyTooLarge);
        let error = Error::from(kind);
        assert_eq!(
            error.code(),
            afd_core::error_code::INTERNAL_OPERATION_FAILED
        );
        assert_eq!(error.is_oversize(), size_failure);
    }
    let invalid = Error::from(ErrorKind::Body);
    assert_eq!(invalid.code(), afd_core::error_code::INVALID_REQUEST);
    assert!(!invalid.is_oversize());
}
