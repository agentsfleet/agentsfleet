use afr_egress::fixture::{PUSHOVER_TOKEN, PUSHOVER_USER};
use afr_egress::testing::Sent;
use serde_json::{Value, json};

use super::{MESSAGES_URL, Pushover};
use crate::handler::Typed;
use crate::runtime::{ToolErrorCode, ToolOutput};
use crate::testing::{Run, call, replying};

async fn notify(run: &Run, arguments: Value) -> (ToolOutput, Vec<Sent>) {
    let (transport, sent) = replying(200, r#"{"status":1}"#);
    let tool = Typed::boxed(Pushover::new(transport));
    let lease = run.lease();
    let output = call(tool.as_ref(), &lease, arguments).await;
    (output, sent.try_iter().collect())
}

#[tokio::test]
async fn test_pushover_takes_credentials_from_secrets_map() {
    let run = Run::new(false);

    let (output, sent) = notify(&run, json!({"message": "deploy finished", "priority": 1})).await;

    assert_eq!(output.error_code, None);
    let [request] = sent.as_slice() else {
        unreachable!("one request should have been sent, not {}", sent.len());
    };
    assert_eq!(
        (request.method.as_str(), request.url.as_str()),
        ("POST", MESSAGES_URL)
    );
    assert_eq!(request.header("content-type"), Some("application/json"));
    let body: Value =
        serde_json::from_str(request.body.as_deref().unwrap_or_default()).unwrap_or_default();
    assert_eq!(
        body,
        json!({"token": PUSHOVER_TOKEN, "user": PUSHOVER_USER, "message": "deploy finished", "priority": 1})
    );

    for refused in [
        json!({"message": "x", "token": "model_supplied"}),
        json!({"message": "x", "user": "model_supplied"}),
        json!({"message": "x", "priority": 3}),
    ] {
        let (output, sent) = notify(&run, refused).await;
        assert_eq!(output.error_code, Some(ToolErrorCode::InvalidArguments));
        assert_eq!(sent, Vec::new());
    }
}

#[tokio::test]
async fn should_refuse_when_the_fleet_has_no_pushover_secret() {
    let mut run = Run::new(false);
    run.policy.secrets_map = Some(json!({}));

    let (output, sent) = notify(&run, json!({"message": "x"})).await;

    assert_eq!(output.error_code, Some(ToolErrorCode::SecretNotFound));
    assert_eq!(sent, Vec::new());
}
