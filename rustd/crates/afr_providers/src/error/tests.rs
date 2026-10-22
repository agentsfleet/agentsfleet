#![expect(
    clippy::unwrap_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use afd_core::error_code;
use afd_wire::policy::CUSTOM_PROVIDER_PREFIX;
use afd_wire::report::FailureClass;
use http::StatusCode;
use rig_core::ProviderError;
use rig_core::http_client as rig_http;

use super::{Error, raise};

#[test]
fn should_name_a_refusals_status_in_its_detail_with_no_class() {
    let refused = Error::refused(401);

    assert!(refused.detail().contains("401"), "{}", refused.detail());
    assert_eq!(refused.failure_class(), None);
    assert_eq!(refused.code(), error_code::INTERNAL_OPERATION_FAILED);
}

#[test]
fn should_class_a_lost_connection_and_an_early_end_as_transport_loss() {
    let lost = Error::lost(std::io::Error::other("reset"));
    let ended = raise::ended("overloaded_error", ProviderError::Truncated);

    assert_eq!(lost.failure_class(), Some(FailureClass::TransportLoss));
    assert_eq!(ended.failure_class(), Some(FailureClass::TransportLoss));
    assert!(
        ended.detail().ends_with("overloaded_error"),
        "{}",
        ended.detail()
    );
}

#[test]
fn should_refuse_an_unknown_provider_as_the_fleets_configuration() {
    let refused = raise::unhosted("groq");

    assert_eq!(refused.unhosted_provider(), Some("groq"));
    assert_eq!(refused.code(), error_code::AGENTSFLEET_INVALID_CONFIG);
    assert_eq!(refused.failure_class(), None);
    assert_eq!(Error::refused(500).unhosted_provider(), None);
}

/// rig's reply for `status` with `body`, as its decoders preserve it.
fn answered(status: u16, body: &str) -> ProviderError {
    ProviderError::from_http_response(StatusCode::from_u16(status).unwrap(), body)
}

#[test]
fn should_refuse_an_answered_status_under_the_providers_own_code() {
    let named = raise::provider(answered(
        400,
        r#"{"error":{"code":"context_length_exceeded"}}"#,
    ));
    let unnamed = raise::provider(answered(503, ""));

    assert!(
        named
            .detail()
            .ends_with("status 400 (context_length_exceeded)"),
        "{}",
        named.detail()
    );
    assert!(
        unnamed.detail().ends_with("status 503"),
        "{}",
        unnamed.detail()
    );
    assert_eq!(named.failure_class(), None);
    assert_eq!(named.code(), error_code::INTERNAL_OPERATION_FAILED);
    assert!(
        std::error::Error::source(&named)
            .unwrap()
            .downcast_ref::<ProviderError>()
            .is_some()
    );
}

// A code is a name; a provider that puts a sentence there may be quoting the
// conversation, so it never reaches the report.
#[test]
fn should_carry_no_code_that_reads_as_a_message() {
    let quoted = r#"{"error":{"code":"your prompt said sk-live-secret"}}"#;
    let long = format!(r#"{{"error":{{"code":"{}"}}}}"#, "x".repeat(65));

    for body in [quoted, long.as_str()] {
        let refused = raise::provider(answered(400, body));
        assert!(
            refused.detail().ends_with("status 400"),
            "{}",
            refused.detail()
        );
    }
}

#[test]
fn should_class_a_cut_reply_and_a_provider_error_mid_turn_as_transport_loss() {
    let cut = raise::provider(ProviderError::Truncated);
    let overloaded = raise::provider(ProviderError::from_provider_body(
        r#"{"error":{"type":"overloaded_error"}}"#,
    ));
    let unnamed = raise::provider(ProviderError::Provider(
        "the prompt said sk-live-secret".into(),
    ));

    for lost in [&cut, &overloaded, &unnamed] {
        assert_eq!(lost.failure_class(), Some(FailureClass::TransportLoss));
        assert!(
            std::error::Error::source(lost)
                .unwrap()
                .downcast_ref::<ProviderError>()
                .is_some()
        );
    }
    assert!(
        overloaded.detail().ends_with("overloaded_error"),
        "{}",
        overloaded.detail()
    );
    assert!(
        unnamed.detail().ends_with("provider_error"),
        "{}",
        unnamed.detail()
    );
}

#[test]
fn should_name_a_reply_the_wire_could_not_read_the_fleets_error_with_its_cause() {
    let unread = raise::provider(ProviderError::Response("no choices".into()));

    assert_eq!(unread.failure_class(), None);
    assert!(
        std::error::Error::source(&unread).is_some(),
        "rig's reason, kept"
    );
}

#[tokio::test]
async fn should_name_a_reply_past_the_cap_the_fleets_error_and_never_reopen_it() {
    use afd_inference::transport::{HttpTransport, REPLY_MAX_BYTES, SendRequest};
    use bytes::Bytes;
    use futures_util::future::BoxFuture;
    use rig_http::HttpClientExt as _;
    #[derive(Debug)]
    struct Oversized;
    impl SendRequest for Oversized {
        fn send(
            &self,
            _: http::Request<Bytes>,
        ) -> BoxFuture<'static, rig_http::Result<rig_http::StreamingResponse>> {
            Box::pin(async {
                Ok(http::Response::new(
                    Box::pin(futures_util::stream::iter([Ok(Bytes::from(vec![
                        b'x';
                        REPLY_MAX_BYTES
                            + 1
                    ]))])) as rig_http::BoxedStream,
                ))
            })
        }
    }
    let response = HttpTransport::new(std::sync::Arc::new(Oversized))
        .send::<_, Bytes>(http::Request::new(Bytes::new()))
        .await
        .unwrap();
    let source = response.into_body().await.unwrap_err();
    let failure = ProviderError::from_transport_error(source);
    let reset = ProviderError::from_transport_error(rig_http::Error::instance(
        std::io::Error::other("reset"),
    ));

    assert!(raise::oversize(&failure));
    let oversized = raise::provider(failure);
    assert!(!raise::oversize(&reset), "a reset is lost, not oversized");
    assert_eq!(oversized.failure_class(), None);
    assert_eq!(oversized.code(), error_code::INTERNAL_OPERATION_FAILED);
    assert!(oversized.detail().contains("byte bound"));
    assert!(std::error::Error::source(&oversized).is_some());
    assert_eq!(
        raise::provider(reset).failure_class(),
        Some(FailureClass::TransportLoss)
    );
}

/// The URL of a self-hosted endpoint at a loopback literal.
const PRIVATE_URL: &str = "https://127.0.0.1/v1";

#[test]
fn should_name_a_blocked_endpoint_as_the_fleets_configuration_and_never_as_unhosted() {
    let endpoint = format!("{CUSTOM_PROVIDER_PREFIX}{PRIVATE_URL}");
    let blocked = raise::blocked_endpoint(&endpoint);

    assert_eq!(blocked.code(), error_code::AGENTSFLEET_INVALID_CONFIG);
    assert_eq!(blocked.failure_class(), None);
    assert_eq!(blocked.unhosted_provider(), None);
    assert!(blocked.detail().contains(&endpoint));
}

// Value: protects=client construction failures keep their typed cause and internal-failure classification;
// fails_when=a builder error is swallowed or reported as a retryable provider connection loss;
// why_new=normal connector construction cannot trigger a deterministic builder refusal;
// seam=the real reqwest builder; no_claim=provider network failure.
#[test]
fn should_preserve_a_client_builder_refusal_without_calling_it_transport_loss() {
    let source = afd_identity::direct(std::time::Duration::from_secs(1))
        .user_agent("\n")
        .build()
        .unwrap_err();
    let refused = raise::client(source);
    assert_eq!(refused.code(), error_code::INTERNAL_OPERATION_FAILED);
    assert_eq!(refused.failure_class(), None);
    assert!(
        std::error::Error::source(&refused)
            .unwrap()
            .downcast_ref::<reqwest::Error>()
            .is_some()
    );
}
