#![expect(
    clippy::panic,
    reason = "test module: a sample with no expectation should fail the test loudly"
)]

use afd_core::error_code::{self, ErrorCode};

use super::one_of_each_kind;
use crate::refusal::Refusal;

/// What each sample is refused as, and the code an operator reads.
fn expected(name: &str) -> (Option<Refusal>, ErrorCode) {
    let refused = |refusal| (Some(refusal), error_code::INVALID_REQUEST);
    match name {
        "client" => (None, error_code::INTERNAL_OPERATION_FAILED),
        "invalid url" => refused(Refusal::InvalidUrl),
        "invalid header" => refused(Refusal::InvalidHeader),
        "https required" => refused(Refusal::HttpsRequired),
        "method not allowed" => refused(Refusal::MethodNotAllowed),
        "host not allowed" => refused(Refusal::HostNotAllowed),
        "address not allowed" => refused(Refusal::AddressNotAllowed),
        "placement not allowed" => refused(Refusal::PlacementNotAllowed),
        "credential host not allowed" => refused(Refusal::CredentialHostNotAllowed),
        "request policy not allowed" => refused(Refusal::RequestPolicyNotAllowed),
        "secret not found" => (Some(Refusal::SecretNotFound), error_code::SECRET_NOT_FOUND),
        "mint refused" => (
            Some(Refusal::CredentialMintRefused),
            error_code::GH_MINT_FAILED,
        ),
        "upstream unreachable" => (
            Some(Refusal::UpstreamUnreachable),
            error_code::INTERNAL_OPERATION_FAILED,
        ),
        unknown => panic!("no expectation for the {unknown} sample"),
    }
}

#[test]
fn should_name_each_kind_its_refusal_and_its_code() {
    for (name, error) in one_of_each_kind() {
        assert_eq!((error.refusal(), error.code()), expected(name), "{name}");
    }
}

// The model reads the detail; an operator reads the `[CODE]` rendering.
#[test]
fn should_hand_the_model_its_sentence_without_the_code() {
    for (name, error) in one_of_each_kind() {
        let detail = error.detail();
        assert!(!detail.starts_with('['), "{name}: {detail}");
        assert_eq!(
            error.to_string(),
            format!("[{}] {detail}", error.code()),
            "{name}"
        );
    }
}

#[test]
fn should_keep_the_cause_the_model_needs_in_its_sentence() {
    let sample = |wanted: &str| {
        one_of_each_kind()
            .into_iter()
            .find_map(|(name, error)| (name == wanted).then(|| error.detail()))
    };

    assert_eq!(
        sample("invalid url").as_deref(),
        Some("the URL does not parse: empty host")
    );
    assert_eq!(
        sample("mint refused").as_deref(),
        Some("the credential could not be minted: d")
    );
}

// A refusal the run cannot change tells the model so in the same sentence,
// or the model spends a turn per guess: the 167K-token run on dev (event
// 1791777420543-1356) retried one refused review post sixteen ways.
#[test]
fn should_tell_the_model_a_fixed_refusal_is_not_worth_retrying() {
    let sample = |wanted: &str| {
        one_of_each_kind()
            .into_iter()
            .find_map(|(name, error)| (name == wanted).then(|| error.detail()))
    };

    let policy = sample("request policy not allowed").unwrap_or_default();
    assert!(policy.starts_with("no request rule at "), "{policy}");
    assert!(policy.contains("fixed before it starts"), "{policy}");
    assert!(policy.ends_with("or with another credential"), "{policy}");

    let secret = sample("secret not found").unwrap_or_default();
    assert!(secret.starts_with("this fleet has no secret "), "{secret}");
    assert!(
        secret.ends_with("do not retry with another secret name"),
        "{secret}"
    );
}
