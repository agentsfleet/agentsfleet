//! The daemon-only secret-bearing types redact on `Debug` and still serialize.
//!
//! The same two halves `afd_wire`'s redaction suite proves for the shared
//! types, for the daemon-only types that carry a secret.
#![expect(
    clippy::unwrap_used,
    reason = "test target: a serialization failure should fail the test loudly"
)]

use std::borrow::Cow;

use afd_api_wire::admin::RunnerTokenRotatedResponse;
use afd_api_wire::secret::{ReplaceSecretRequest, StoreSecretRequest};
use afd_api_wire::tenant::{MintedApiKeyResponse, MintedCliCredentialResponse};

/// A value no legitimate field would contain, so finding it anywhere in a
/// rendered string is unambiguous evidence of a leak.
const SECRET: &str = "sk-live-CANARY-must-never-appear-in-a-log";

#[test]
fn should_not_leak_the_rotated_runner_token_through_debug() {
    let response = RunnerTokenRotatedResponse {
        id: Cow::Borrowed("runner"),
        runner_token: Cow::Borrowed(SECRET),
    };
    let rendered = format!("{response:?}");
    assert!(
        !rendered.contains(SECRET),
        "runner token leaked: {rendered}"
    );
    assert!(
        rendered.contains("runner"),
        "the identifier must stay readable"
    );
    assert!(serde_json::to_string(&response).unwrap().contains(SECRET));
}

#[test]
fn should_not_leak_a_secret_body_through_debug_and_still_read_it_verbatim() {
    let body = format!(r#"{{"name":"openai","data":{{"api_key":"{SECRET}"}}}}"#);
    let stored: StoreSecretRequest<'_> = serde_json::from_str(&body).unwrap();
    let rendered = format!("{stored:?}");
    assert!(!rendered.contains(SECRET), "secret body leaked: {rendered}");
    assert!(rendered.contains("openai") && rendered.contains("<redacted>"));
    assert!(stored.data.get().contains(SECRET));

    let body = format!(r#"{{"data":{{"api_key":"{SECRET}"}}}}"#);
    let replaced: ReplaceSecretRequest<'_> = serde_json::from_str(&body).unwrap();
    let rendered = format!("{replaced:?}");
    assert!(!rendered.contains(SECRET), "secret body leaked: {rendered}");
    assert!(replaced.data.get().contains(SECRET));
}

#[test]
fn should_not_leak_a_minted_key_or_credential_through_debug() {
    let key = MintedApiKeyResponse {
        id: Cow::Borrowed("key-id"),
        key_name: Cow::Borrowed("ci"),
        key: Cow::Borrowed(SECRET),
        created_at: jiff::Timestamp::constant(1, 0),
    };
    let rendered = format!("{key:?}");
    assert!(!rendered.contains(SECRET), "api key leaked: {rendered}");
    assert!(rendered.contains("key-id") && rendered.contains("ci"));
    assert!(serde_json::to_string(&key).unwrap().contains(SECRET));

    let credential = MintedCliCredentialResponse {
        id: Cow::Borrowed("credential-id"),
        credential: Cow::Borrowed(SECRET),
        machine_name: Cow::Borrowed("laptop"),
        deployment: Cow::Borrowed("agentsfleet.test"),
    };
    let rendered = format!("{credential:?}");
    assert!(!rendered.contains(SECRET), "credential leaked: {rendered}");
    assert!(rendered.contains("laptop") && rendered.contains("agentsfleet.test"));
    assert!(serde_json::to_string(&credential).unwrap().contains(SECRET));
}
