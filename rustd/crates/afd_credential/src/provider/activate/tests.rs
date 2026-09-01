//! The activation ladder's unit cases.
//!
//! A file module rather than an inline `mod tests`, because the ladder plus
//! its cases crossed the file cap. The cases are unchanged.

use super::effective_model;

/// What a client sends when it is changing model as well as credential.
const OVERRIDE: &str = "claude-opus-5";

/// What an older credential still carries in its body.
const FROM_CREDENTIAL: &str = "claude-sonnet-5";

#[test]
fn the_override_wins_and_the_credential_is_the_fallback() {
    assert_eq!(
        effective_model(Some(OVERRIDE), Some(FROM_CREDENTIAL)),
        Some(OVERRIDE)
    );
    assert_eq!(
        effective_model(None, Some(FROM_CREDENTIAL)),
        Some(FROM_CREDENTIAL)
    );
    assert_eq!(effective_model(Some(OVERRIDE), None), Some(OVERRIDE));
}

#[test]
fn naming_no_model_at_all_is_not_a_model() {
    assert_eq!(effective_model(None, None), None);
}

#[test]
fn a_blank_or_padded_model_is_refused_rather_than_trimmed() {
    // Trimming would store a name the caller did not type and hide the
    // typo; both sources are held to it.
    for refused in [
        "",
        "   ",
        " claude-opus-5",
        "claude-opus-5 ",
        "\tclaude-opus-5",
    ] {
        assert_eq!(effective_model(Some(refused), None), None, "{refused:?}");
        assert_eq!(effective_model(None, Some(refused)), None, "{refused:?}");
    }
}

#[test]
fn a_blank_override_does_not_fall_through_to_the_credential() {
    // The Zig computes `input.model orelse probed.model` and then checks
    // the RESULT, so an empty override is a refusal rather than a reason
    // to use the credential's. Kept: a client that sent a field meant it.
    assert_eq!(effective_model(Some(""), Some(FROM_CREDENTIAL)), None);
}
