use std::sync::Arc;

use afd_core::test_util::trace::Capture;
use afr_egress::error::one_of_each_kind;
use afr_egress::fixture::{BRANCH, ELASTIC_QUERY, GRAFANA, GRAFANA_TOKEN, LEASE_ID};
use afr_egress::testing::{RecordingTransport, Sent, inbound};
use afr_egress::{Inbound, Outbound, RESPONSE_MAX_BYTES, Transport};
use serde_json::{Value, json};
use tokio::sync::Notify;

use super::HttpRequest;
use crate::egress::SharedTransport;
use crate::handler::Typed;
use crate::runtime::{ToolErrorCode, ToolOutput};
use crate::testing::{HOUR, MINTED, Run, call, replying};

const REFS: &str = "https://api.github.com/repos/acme/widgets/git/refs";
/// The body every upstream in this suite answers with.
const ANSWERED: &str = "ok";
const GITHUB_AUTH: &str = "Bearer ${secrets.github.token}";

/// One call under the fixture policy with `read_only`, and every request
/// that reached the transport.
async fn request(read_only: bool, arguments: Value) -> (ToolOutput, Vec<Sent>) {
    let (transport, sent) = replying(200, ANSWERED);
    let output = request_through(read_only, transport, arguments).await;
    (output, sent.try_iter().collect())
}

/// One call under the fixture policy, sent through `transport`.
async fn request_through(
    read_only: bool,
    transport: SharedTransport,
    arguments: Value,
) -> ToolOutput {
    let run = Run::new(read_only);
    let tool = Typed::boxed(HttpRequest::new(transport));
    let lease = run.lease();
    call(tool.as_ref(), &lease, arguments).await
}

#[tokio::test]
async fn test_http_request_refuses_unlisted_host() {
    let capture = Capture::install();
    let (output, sent) = request(false, json!({"url": "https://evil.example/exfil"})).await;

    assert_eq!(output.error_code, Some(ToolErrorCode::HostNotAllowed));
    assert_eq!(sent, Vec::new());
    let refused = capture.only("tool_refused");
    assert_eq!(refused.field("lease_id"), Some(LEASE_ID));
    assert_eq!(refused.field("tool"), Some("http_request"));
    assert_eq!(refused.field("error_code"), Some("host_not_allowed"));
}

#[tokio::test]
async fn test_http_request_read_only_admits_listed_posts() {
    let (listed, sent) = request(
        true,
        json!({"url": ELASTIC_QUERY, "method": "POST", "body": "{}"}),
    )
    .await;
    assert_eq!(listed.error_code, None);
    assert_eq!(
        sent.iter()
            .map(|request| request.url.as_str())
            .collect::<Vec<_>>(),
        [ELASTIC_QUERY]
    );

    let (other, sent) = request(
        true,
        json!({"url": "https://demo.es.example/_bulk", "method": "POST", "body": "{}"}),
    )
    .await;
    assert_eq!(other.error_code, Some(ToolErrorCode::MethodNotAllowed));
    assert_eq!(sent, Vec::new());
}

#[tokio::test]
async fn test_http_request_enforces_origin_rules() {
    let headers = json!({"Authorization": GITHUB_AUTH});
    let other_ref = json!({"url": REFS, "method": "POST", "headers": headers, "body": r#"{"ref":"refs/heads/main","sha":"abc"}"#});
    let (refused, sent) = request(false, other_ref).await;
    assert_eq!(
        refused.error_code,
        Some(ToolErrorCode::RequestPolicyNotAllowed)
    );
    assert_eq!(sent, Vec::new());

    let locked = format!(r#"{{"ref":"refs/heads/{BRANCH}","sha":"abc"}}"#);
    let (admitted, sent) = request(
        false,
        json!({"url": REFS, "method": "POST", "headers": headers, "body": locked}),
    )
    .await;
    assert_eq!(admitted.error_code, None);
    assert_eq!(sent.len(), 1);
    assert_eq!(
        sent.first()
            .and_then(|request| request.header("authorization")),
        Some(format!("Bearer {MINTED}").as_str())
    );
}

#[tokio::test]
async fn test_placeholder_substituted_only_in_authorization() {
    let in_header = json!({"url": "https://demo-grafana.internal/api/search", "headers": {"Authorization": "Bearer ${secrets.grafana.token}"}});
    let (_output, sent) = request(false, in_header).await;
    assert_eq!(
        sent.first()
            .and_then(|request| request.header("authorization")),
        Some(format!("Bearer {GRAFANA_TOKEN}").as_str())
    );

    let (_output, sent) = request(
        false,
        json!({"url": "https://${secrets.grafana.host}/api/health"}),
    )
    .await;
    assert_eq!(
        sent.first().map(|request| request.url.clone()),
        Some(format!("https://{GRAFANA}/api/health"))
    );

    let in_url = json!({"url": "https://demo-grafana.internal/api?key=${secrets.grafana.token}"});
    let in_body = json!({"url": "https://demo-grafana.internal/api", "method": "POST", "body": "{\"key\":\"${secrets.grafana.token}\"}"});
    for misplaced in [in_url, in_body] {
        let (output, sent) = request(false, misplaced).await;
        assert_eq!(
            output.error_code,
            Some(ToolErrorCode::CredentialPlacementNotAllowed)
        );
        assert_eq!(sent, Vec::new());
    }
}

#[tokio::test]
async fn test_mintable_credential_minted_once() {
    let capture = Capture::install();
    let run = Run::new(false);
    let (transport, sent) = replying(200, "[]");
    let tool = Typed::boxed(HttpRequest::new(transport));
    let lease = run.lease();
    let read = json!({"url": "https://api.github.com/repos/acme/widgets/pulls", "headers": {"Authorization": GITHUB_AUTH}});

    for _call in 0..3 {
        let output = call(tool.as_ref(), &lease, read.clone()).await;
        assert_eq!(output.error_code, None);
    }
    assert_eq!(run.mint.asked(), 1);

    run.clock.advance_millis(HOUR);
    let output = call(tool.as_ref(), &lease, read).await;
    assert_eq!(output.error_code, None);
    assert_eq!(run.mint.asked(), 2);
    assert_eq!(sent.try_iter().count(), 4);
    let minted: Vec<_> = (capture.events().into_iter())
        .filter(|event| event.field("event") == Some("credential_minted"))
        .collect();
    assert_eq!(minted.len(), 2);
    assert!(
        minted
            .iter()
            .all(|event| event.field("lease_id") == Some(LEASE_ID))
    );
    assert!(
        (capture.events().iter()).all(|event| event.fields.values().all(|v| !v.contains(MINTED))),
        "no log line carries the minted token"
    );
}

/// A transport failure that names no refusal — the client itself broke — is
/// still an answer the model can act on: unreachable, under that code, and
/// never a bare registry string.
#[tokio::test]
async fn should_hand_a_transport_failure_back_as_upstream_unreachable() {
    let (transport, _sent) = RecordingTransport::answering(|_outbound| {
        let Some((_name, failure)) = (one_of_each_kind().into_iter())
            .find(|(name, failure)| *name == "client" && failure.refusal().is_none())
        else {
            unreachable!("the client failure is one of each kind and no refusal")
        };
        Err(failure)
    });

    let output = request_through(
        false,
        Arc::new(transport),
        json!({"url": "https://api.github.com/repos/acme/widgets/"}),
    )
    .await;

    assert_eq!(output.error_code, Some(ToolErrorCode::UpstreamUnreachable));
    assert!(
        output.text.starts_with("[upstream_unreachable] "),
        "{}",
        output.text
    );
}

#[tokio::test]
async fn should_mask_a_minted_token_the_upstream_echoes() {
    let run = Run::new(false);
    let (transport, _sent) =
        RecordingTransport::answering(|_outbound| Ok(inbound(200, &format!("token={MINTED}"))));
    let tool = Typed::boxed(HttpRequest::new(Arc::new(transport)));
    let lease = run.lease();

    let output = call(
        tool.as_ref(),
        &lease,
        json!({"url": "https://api.github.com/repos/acme/widgets/", "headers": {"Authorization": GITHUB_AUTH}}),
    )
    .await;

    assert_eq!(
        output.text,
        "Status: 200\n\nResponse Body:\ntoken=«secret:github.token»"
    );
}

#[tokio::test]
async fn should_answer_a_status_outside_2xx_as_failed_with_where_a_redirect_points() {
    let reply = Inbound {
        status: 302,
        location: Some("https://logs.example.net".to_owned()),
        content_type: None,
        body: String::new(),
        truncated: true,
    };
    let (transport, _sent) = RecordingTransport::answering(move |_outbound| Ok(reply.clone()));

    let output = request_through(
        false,
        Arc::new(transport),
        json!({"url": "https://api.github.com/repos/acme/widgets/actions/jobs/7/logs"}),
    )
    .await;

    assert_eq!(output.error_code, Some(ToolErrorCode::UpstreamStatus));
    assert_eq!(
        output.text,
        format!(
            "[upstream_status] Status: 302\nLocation: https://logs.example.net\n\nResponse Body:\n\n\
             [Response truncated at {RESPONSE_MAX_BYTES} bytes]"
        )
    );
}

#[tokio::test]
async fn should_refuse_arguments_the_schema_does_not_name() {
    let (output, sent) = request(false, json!({"url": ELASTIC_QUERY, "token": "x"})).await;

    assert_eq!(output.error_code, Some(ToolErrorCode::InvalidArguments));
    assert_eq!(sent, Vec::new());
}

/// A transport that says when a request reaches it and answers only when
/// released, so a test can look at the lease while the request is in flight.
#[derive(Debug, Default)]
struct Gated {
    sending: Notify,
    release: Notify,
}

#[async_trait::async_trait]
impl Transport for Gated {
    async fn send(&self, _outbound: Outbound) -> afr_egress::Result<Inbound> {
        self.sending.notify_one();
        self.release.notified().await;
        Ok(inbound(200, ANSWERED))
    }
}

/// The guard is held to admit and mint, never across the send: a slow
/// upstream leaves every other call of the lease free to admit its own.
#[tokio::test]
async fn should_leave_the_guard_free_while_a_request_is_in_flight() {
    let run = Run::new(false);
    let gated = Arc::new(Gated::default());
    let transport: SharedTransport = Arc::clone(&gated) as SharedTransport;
    let tool = Typed::boxed(HttpRequest::new(transport));
    let lease = run.lease();
    let probe = async {
        gated.sending.notified().await;
        let free = lease.egress.try_lock().is_ok();
        gated.release.notify_one();
        free
    };

    let (output, free) = tokio::join!(
        call(
            tool.as_ref(),
            &lease,
            json!({"url": "https://api.github.com/repos/acme/widgets/"})
        ),
        probe
    );

    assert!(free, "the guard was held while the request was in flight");
    assert_eq!(output.error_code, None, "{output:?}");
}
