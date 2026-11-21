//! A wire failure names what failed and keeps its typed cause, and the
//! predicates an embedder classifies it by read through rig's wrapping.

use std::error::Error as _;

use rig_core::ProviderError;
use rig_core::http_client as rig_http;

use super::{Error, ErrorKind, Failure};
use crate::wire::transport::SendError;

/// rig's failure for a transport failure `source`.
fn through_rig(source: impl std::error::Error + Send + Sync + 'static) -> ProviderError {
    ProviderError::from_transport_error(rig_http::Error::permanent(source))
}

#[test]
fn provider_failures_keep_their_classification_and_size_identity() {
    for kind in [
        ErrorKind::ReplyTooLarge,
        ErrorKind::StreamDeadline,
        ErrorKind::StreamCancelled,
    ] {
        let size_failure = matches!(kind, ErrorKind::ReplyTooLarge);
        let error = Error::from(kind);
        assert!(!error.is_invalid_request());
        assert_eq!(error.is_reply_too_large(), size_failure);
    }
    let invalid = Error::from(ErrorKind::Body);
    assert!(invalid.is_invalid_request());
    assert!(!invalid.is_reply_too_large());
}

#[test]
fn a_bound_rig_carried_is_still_read_as_the_bound() {
    let failure = Error::provider(through_rig(Error::from(ErrorKind::ReplyTooLarge)));

    assert!(failure.is_reply_too_large());
    assert!(!failure.is_refused());
    assert_eq!(failure.status(), None);
    assert_eq!(failure.to_string(), "the model provider failed the turn");
}

#[test]
fn a_destination_the_guard_refused_is_read_as_refused() {
    let refusal = SendError::Refused(Box::new(std::io::Error::other("blocked")));
    let failure = Error::provider(through_rig(refusal));

    assert!(failure.is_refused());
    assert!(!failure.is_reply_too_large());
    assert_eq!(failure.failure(), Some(Failure::Lost));
    assert!(
        failure
            .into_cause()
            .is_ok_and(|cause| matches!(cause.downcast_ref(), Some(ProviderError::Http(_))))
    );
}

#[test]
fn a_refusal_names_its_status_and_nothing_the_provider_wrote() {
    let failure = Error::provider(ProviderError::from_http_response(
        http::StatusCode::TOO_MANY_REQUESTS,
        r#"{"error":{"message":"your prompt said sk-live-secret"}}"#,
    ));

    assert_eq!(failure.status(), Some(429));
    assert_eq!(
        failure.to_string(),
        "the model provider refused the turn with status 429"
    );
    assert!(!format!("{failure}").contains("sk-live-secret"));
}

#[test]
fn a_failure_that_is_not_the_providers_is_handed_back_unchanged() {
    let failure = Error::unsendable("call-1");

    assert_eq!(failure.failure(), None);
    assert_eq!(failure.provider_code(), None);
    let back = failure.into_cause().err();

    assert!(back.as_ref().is_some_and(Error::is_unsendable));
    assert_eq!(back.as_ref().and_then(Error::status), None);
}

#[test]
fn a_provider_failure_is_named_by_what_failed_and_the_providers_code() {
    let answered = Error::provider(ProviderError::from_http_response(
        http::StatusCode::BAD_REQUEST,
        r#"{"error":{"code":"context_length_exceeded"}}"#,
    ));
    let ended = Error::provider(ProviderError::from_provider_body(
        r#"{"error":{"type":"overloaded_error"}}"#,
    ));

    assert_eq!(answered.failure(), Some(Failure::Refused { status: 400 }));
    assert_eq!(
        answered.provider_code().as_deref(),
        Some("context_length_exceeded")
    );
    assert_eq!(ended.failure(), Some(Failure::Ended));
    assert_eq!(ended.provider_code().as_deref(), Some("overloaded_error"));
    for (failure, kind) in [
        (ProviderError::Truncated, Failure::Lost),
        (ProviderError::Provider("diagnostic".into()), Failure::Ended),
        (
            ProviderError::Response("no choices".into()),
            Failure::Unread,
        ),
    ] {
        let failure = Error::provider(failure);
        assert_eq!(failure.failure(), Some(kind));
        assert_eq!(failure.provider_code(), None);
    }
}

#[test]
fn the_kind_is_the_text_and_its_source_the_cause() {
    let source = serde_json::from_str::<serde_json::Value>("{").err();
    let failure = source.map(Error::unreadable);

    assert!(failure.as_ref().is_some_and(Error::is_unsendable));
    assert!(
        failure
            .as_ref()
            .and_then(|failure| failure.source())
            .is_some_and(<dyn std::error::Error>::is::<serde_json::Error>)
    );
    assert!(failure.is_some_and(|failure| format!("{failure:?}").contains("Unreadable")));
}
