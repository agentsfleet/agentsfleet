#![expect(
    clippy::expect_used,
    reason = "invalid fixture behavior must fail loudly"
)]

use super::validate_key;
use afd_core::error_code;
use afd_wire::inference::InferenceApi;

const APIS: [InferenceApi; 5] = [
    InferenceApi::Chat,
    InferenceApi::Responses,
    InferenceApi::Messages,
    InferenceApi::Gemini,
    InferenceApi::Ollama,
];

#[test]
fn should_accept_valid_header_keys_when_any_supported_api_is_selected() {
    for api in APIS {
        validate_key(api, "fixture-key").expect("valid key");
    }
}

#[test]
fn should_reject_without_private_bytes_when_key_contains_raw_control_characters() {
    for api in APIS {
        for key in ["fixture-key\n", "fixture-key\r", "fixture-key\0"] {
            let error = validate_key(api, key).expect_err("invalid header");
            assert_eq!(error.code(), error_code::INVALID_REQUEST);
            assert!(!error.to_string().contains("fixture-key"));
            assert!(!format!("{error:?}").contains("fixture-key"));
        }
    }
}

#[test]
fn should_validate_decoded_header_when_gemini_key_contains_escaped_controls() {
    for key in [
        "fixture-key%0A",
        "fixture-key%0D",
        "fixture-key%00",
        "fixture-key&key=second%0A",
    ] {
        let error = validate_key(InferenceApi::Gemini, key).expect_err("decoded header is invalid");
        assert_eq!(error.code(), error_code::INVALID_REQUEST);
        assert!(!format!("{error:?}").contains("fixture-key"));
    }
    validate_key(InferenceApi::Chat, "fixture-key%0A").expect("other APIs do not decode keys");
    validate_key(InferenceApi::Gemini, "fixture%2Dkey").expect("valid encoded key");
    validate_key(InferenceApi::Gemini, "fixture+key").expect("query space is a valid header value");
}

#[test]
fn should_reject_invalid_query_syntax_when_gemini_key_cannot_enter_the_request_uri() {
    let error = validate_key(InferenceApi::Gemini, "fixture key").expect_err("unescaped URI space");
    assert_eq!(error.code(), error_code::INVALID_REQUEST);
    assert!(!format!("{error:?}").contains("fixture key"));
}
