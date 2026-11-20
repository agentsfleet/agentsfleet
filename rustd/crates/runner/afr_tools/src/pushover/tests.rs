//! Pushover as the run's notifier: `guffbolt`'s `notify`, built over this
//! backend, sending through the run's guard, so what a fleet's run reads is
//! proved here.

#![expect(
    clippy::expect_used,
    reason = "test module: a tool the run does not build should fail the test loudly"
)]

use af_egress::fixture::{PUSHOVER, PUSHOVER_TOKEN, PUSHOVER_USER};
use af_egress::testing::Sent;
use guffbolt::ToolOutput;
use serde_json::{Value, json};

use super::MESSAGES_URL;
use crate::backed::Backends;
use crate::runtime::ToolErrorCode;
use crate::testing::{RunFixture, call, replying};

/// The tool a fleet calls to notify its person.
const NOTIFY: &str = "notify";

/// Calls `notify` with `arguments` on `run`, Pushover answering `status`.
async fn notify(run: &RunFixture, status: u16, arguments: Value) -> (ToolOutput, Vec<Sent>) {
    let (transport, sent) = replying(status, r#"{"status":1}"#);
    let run = run.run();
    let backends = Backends::new(&run, Some(transport.as_ref()));
    let tools = backends.tools(&[NOTIFY]).expect("a run builds notify");
    let tool = tools.tool(NOTIFY).expect("notify is built");
    let output = call(tool, &run, arguments).await;
    (output, sent.try_iter().collect())
}

/// The one body a sent request carried, as JSON.
fn body_of(request: &Sent) -> Value {
    serde_json::from_str(request.body.as_deref().unwrap_or_default()).unwrap_or_default()
}

#[tokio::test]
async fn pushover_backend_sends_through_the_egress_guard() {
    let run = RunFixture::new(false);

    let (output, sent) = notify(
        &run,
        200,
        json!({"message": "deploy finished", "title": "Prod", "priority": "high"}),
    )
    .await;

    assert_eq!(output.error_code, None, "{}", output.text);
    assert_eq!(output.text, "Status: 200\n\nResponse Body:\n{\"status\":1}");
    let [request] = sent.as_slice() else {
        unreachable!("one request should have been sent, not {}", sent.len());
    };
    assert_eq!(
        (request.method.as_str(), request.url.as_str()),
        ("POST", MESSAGES_URL)
    );
    assert_eq!(request.header("content-type"), Some("application/json"));
    assert_eq!(
        body_of(request),
        json!({"token": PUSHOVER_TOKEN, "user": PUSHOVER_USER, "message": "deploy finished",
               "title": "Prod", "priority": 1})
    );
}

/// A run whose policy does not list Pushover's host cannot notify: the guard
/// refuses before anything leaves.
#[tokio::test]
async fn a_run_whose_policy_lacks_pushover_cannot_notify() {
    let mut run = RunFixture::new(false);
    run.policy
        .network_policy
        .allow
        .retain(|host| host.as_ref() != PUSHOVER);

    let (output, sent) = notify(&run, 200, json!({"message": "x"})).await;

    assert_eq!(
        output.error_code,
        Some(ToolErrorCode::HostNotAllowed.into())
    );
    assert_eq!(sent, Vec::new());
}

#[tokio::test]
async fn a_low_priority_is_quiet_and_normal_names_none() {
    let run = RunFixture::new(false);

    let (_output, low) = notify(&run, 200, json!({"message": "x", "priority": "low"})).await;
    let (_output, normal) = notify(&run, 200, json!({"message": "x"})).await;

    let low = low.first().map_or_default(body_of);
    let normal = normal.first().map_or_default(body_of);
    assert_eq!(low.get("priority"), Some(&json!(-1)), "{low}");
    assert_eq!(normal.get("priority"), None, "{normal}");
}

#[tokio::test]
async fn the_model_names_no_credential_and_no_vendor_priority() {
    let run = RunFixture::new(false);

    for refused in [
        json!({"message": "x", "token": "model_supplied"}),
        json!({"message": "x", "user": "model_supplied"}),
        json!({"message": "x", "priority": 2}),
        json!({"message": "x", "priority": "emergency"}),
        json!({"message": "x", "sound": "siren"}),
        json!({}),
    ] {
        let (output, sent) = notify(&run, 200, refused.clone()).await;
        assert_eq!(
            output.error_code,
            Some(ToolErrorCode::InvalidArguments.into()),
            "{refused}"
        );
        assert_eq!(sent, Vec::new(), "{refused}");
    }
}

#[tokio::test]
async fn a_fleet_without_a_pushover_secret_sends_nothing() {
    let mut run = RunFixture::new(false);
    run.policy.secrets_map = Some(json!({}));

    let (output, sent) = notify(&run, 200, json!({"message": "x"})).await;

    assert_eq!(
        output.error_code,
        Some(ToolErrorCode::SecretNotFound.into())
    );
    assert_eq!(sent, Vec::new());
}

#[tokio::test]
async fn a_pushover_refusal_reads_as_upstream_status() {
    let run = RunFixture::new(false);

    let (output, sent) = notify(&run, 400, json!({"message": "x"})).await;

    assert_eq!(
        output.error_code,
        Some(ToolErrorCode::UpstreamStatus.into())
    );
    assert!(output.text.contains("Status: 400"), "{}", output.text);
    assert_eq!(sent.len(), 1, "the request left, and Pushover refused it");
}

/// A catalog with no transport builds a notifier that sends nothing: a run
/// hosted that way cannot notify, and says why.
#[tokio::test]
async fn a_notifier_without_a_transport_refuses() {
    let fixture = RunFixture::new(false);
    let run = fixture.run();
    let backends = Backends::new(&run, None);
    let tools = backends.tools(&[NOTIFY]).expect("a run builds notify");
    let tool = tools.tool(NOTIFY).expect("notify is built");

    let output = call(tool, &run, json!({"message": "x"})).await;

    assert_eq!(
        output.error_code,
        Some(ToolErrorCode::UpstreamUnreachable.into())
    );
}
