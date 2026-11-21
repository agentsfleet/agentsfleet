#![expect(clippy::unwrap_used, reason = "fixtures fail loudly on invalid setup")]

use super::*;
use std::error::Error as _;

fn settings(api: InferenceApi) -> ModelInference {
    ModelInference {
        api,
        base_url: "https://inference.example/custom/v1".into(),
        dialect: None,
        max_output_tokens: 37,
    }
}

#[test]
fn rejects_unsafe_endpoints_without_exposing_them() {
    for endpoint in [
        "not a url",
        "http://example.com",
        "file:///etc/passwd",
        "https://user:secret@example.com",
        "https://example.com?secret=value",
        "https://example.com#secret",
        "https://127.0.0.1",
        "https://169.254.169.254",
        "https://[::1]",
        "https://[::ffff:10.0.0.1]",
    ] {
        let mut value = settings(InferenceApi::Chat);
        value.base_url = endpoint.into();
        let error = Configured::new(&value, 100).unwrap_err();
        assert_eq!(error.code(), af_core::error_code::INVALID_REQUEST);
        assert!(!error.to_string().contains("secret"));
    }
    for endpoint in ["https://8.8.8.8", "https://[2606:4700:4700::1111]"] {
        let mut value = settings(InferenceApi::Chat);
        value.base_url = endpoint.into();
        let route = Configured::new(&value, 100).unwrap();
        assert_eq!(
            route.into_host().as_ref(),
            endpoint.trim_start_matches("https://")
        );
    }
}

#[test]
fn enforces_declared_bounds_and_model_specific_output_limit() {
    let mut value = settings(InferenceApi::Chat);
    Configured::new(&value, 37).unwrap();
    assert_eq!(
        Configured::new(&value, 36).unwrap_err().code(),
        af_core::error_code::INVALID_REQUEST
    );
    value.max_output_tokens = 0;
    let error = Configured::new(&value, 100).unwrap_err();
    assert!(error.source().is_some());
    value.max_output_tokens = 37;
    value.base_url = String::new();
    assert_eq!(
        Configured::new(&value, 100).unwrap_err().code(),
        af_core::error_code::INVALID_REQUEST
    );
}

#[test]
fn rejects_unknown_or_wrong_api_dialects_under_the_request_code() {
    for api in [
        InferenceApi::Chat,
        InferenceApi::Responses,
        InferenceApi::Messages,
    ] {
        let mut value = settings(api);
        for dialect in ["not-a-dialect", ""] {
            value.dialect = Some(dialect.into());
            let error = Configured::new(&value, 100).unwrap_err();
            assert_eq!(error.code(), af_core::error_code::INVALID_REQUEST);
        }
        value.dialect = Some("not-a-dialect".into());
        assert_eq!(
            Configured::new(&value, 100).unwrap_err().to_string(),
            "[UZ-REQ-001] inference dialect is not supported for the configured API"
        );
    }
}

#[test]
fn a_credential_endpoint_restriction_cannot_be_replaced_by_catalogue_routing() {
    let value = settings(InferenceApi::Chat);
    let route = Configured::new(&value, 100).unwrap();
    assert!(route.accepts_endpoint(None));
    assert!(route.accepts_endpoint(Some(&value.base_url)));
    assert!(route.accepts_endpoint(Some("https://inference.example/custom/v1/")));
    assert!(!route.accepts_endpoint(Some("https://another.example/v1")));
    assert!(!route.accepts_endpoint(Some("")));
}

#[test]
fn an_owned_route_keeps_validated_routing_after_its_input_is_dropped() {
    let route = {
        let value = settings(InferenceApi::Messages);
        Configured::new(&value, 128_000).unwrap()
    };
    assert!(route.accepts_endpoint(Some("https://inference.example/custom/v1")));
    assert!(!route.accepts_endpoint(Some("https://unrelated.example/v1")));
    assert!(route.supports_tools("configured-model"));
    assert_eq!(route.into_host().as_ref(), "inference.example");
}

#[test]
fn every_saved_api_names_the_wire_of_the_same_name() {
    for (saved, wire) in [
        (InferenceApi::Chat, Api::Chat),
        (InferenceApi::Responses, Api::Responses),
        (InferenceApi::Messages, Api::Messages),
        (InferenceApi::Gemini, Api::Gemini),
        (InferenceApi::Ollama, Api::Ollama),
    ] {
        assert_eq!(api(saved), wire);
        assert_eq!(wire.carries_images(), saved.carries_images());
    }
}

#[test]
fn the_default_timeouts_are_the_existing_provider_behavior() {
    let timeouts = Timeouts::default();
    assert_eq!(timeouts, Timeouts::DEFAULT);
    assert_eq!(timeouts.connect(), CONNECT_TIMEOUT);
    assert_eq!(timeouts.stream_idle(), std::time::Duration::from_secs(300));
    assert_eq!(timeouts.count(), std::time::Duration::from_secs(10));
    assert_eq!(timeouts.count_reply_bytes(), 16 * 1024);
    assert_eq!(timeouts.wire(), guffbolt_ai::wire::Timeouts::DEFAULT);
    let defaults = TimeoutSettings::default();
    assert_eq!(defaults.connect_timeout_seconds, 10);
    assert_eq!(defaults.stream_idle_timeout_seconds, 300);
}

#[test]
fn checked_settings_set_both_deadlines() {
    let settings = garde::Unvalidated::new(TimeoutSettings {
        connect_timeout_seconds: 1,
        stream_idle_timeout_seconds: 2,
    })
    .validate()
    .unwrap();
    let timeouts = Timeouts::new(&settings);
    assert_eq!(timeouts.connect(), std::time::Duration::from_secs(1));
    assert_eq!(timeouts.stream_idle(), std::time::Duration::from_secs(2));
}
