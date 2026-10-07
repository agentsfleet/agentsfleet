//! The daemon-only secret-bearing types redact on `Debug` and still serialize.
//!
//! The same two halves `afd_wire`'s redaction suite proves for the shared
//! types, for the one daemon-only type that carries a secret.
#![expect(
    clippy::unwrap_used,
    reason = "test target: a serialization failure should fail the test loudly"
)]

use std::borrow::Cow;

use afd_api_wire::admin::RunnerTokenRotatedResponse;

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
