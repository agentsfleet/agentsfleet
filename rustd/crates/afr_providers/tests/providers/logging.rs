//! SDK diagnostics cannot print provider-controlled private strings.

use afd_core::test_util::trace::Capture;
use afr_providers::{Connect as _, Message, Request};
use futures_util::StreamExt as _;
use serde_json::json;
use tracing::level_filters::LevelFilter;

use super::support::{Fake, Reply, connector, lease, wires::Wire};

const PRIVATE: &str = "private-provider-content-never-log";

async fn recorded(reply: Reply, filtered: bool) -> String {
    let capture = if filtered {
        Capture::install_filtered(afr_providers::log_filter(LevelFilter::TRACE))
    } else {
        Capture::install()
    };
    let fake = Fake::serve(vec![reply]).await;
    let leased = lease(&Wire::Chat.provider(), &[], "hello");
    let provider = connector(&fake).connect(&leased).unwrap();
    let messages = [Message::User("hello".into())];
    let request = Request {
        model: "configured-model",
        instructions: "Read the run.",
        messages: &messages,
        tools: &[],
        hosted: &[],
        cache_key: "private-log-proof",
    };
    let _chunks: Vec<_> = provider.stream(request).collect().await;
    tracing::warn!(event = "provider_fixture_completed");
    assert_eq!(
        capture.only("provider_fixture_completed").level,
        tracing::Level::WARN
    );
    assert_eq!(
        capture.only("model_turn_started").field("correlation_id"),
        Some(super::support::LEASE_ID)
    );
    format!("{:?}", capture.events())
}

#[tokio::test]
async fn unknown_finish_reasons_are_private_even_at_trace_level() {
    let Reply::Stream(events) = Wire::Chat.answer("answer") else {
        panic!("stream fixture")
    };
    let reply = Reply::Stream(
        events
            .into_iter()
            .map(|event| {
                event.replace(
                    "\"finish_reason\":\"stop\"",
                    &format!("\"finish_reason\":\"{PRIVATE}\""),
                )
            })
            .collect(),
    );
    assert!(
        recorded(reply.clone(), false).await.contains(PRIVATE),
        "control must exercise the SDK warning"
    );
    assert!(!recorded(reply, true).await.contains(PRIVATE));
}

#[tokio::test]
async fn duplicate_call_identifiers_are_private_even_at_trace_level() {
    let Reply::Stream(mut events) = Wire::Chat.call(PRIVATE, "tool", &json!({})) else {
        panic!("stream fixture")
    };
    let mut first: serde_json::Value =
        serde_json::from_str(events[0].strip_prefix("data: ").unwrap().trim()).unwrap();
    let calls = first["choices"][0]["delta"]["tool_calls"]
        .as_array_mut()
        .unwrap();
    let mut duplicate = calls[0].clone();
    duplicate["index"] = json!(1);
    calls.push(duplicate);
    events[0] = format!("data: {first}\n\n");
    let reply = Reply::Stream(events);
    let unfiltered = recorded(reply.clone(), false).await;
    assert!(
        unfiltered.contains("two tool calls with one id"),
        "{unfiltered}"
    );
    assert!(unfiltered.contains(PRIVATE));
    assert!(!recorded(reply, true).await.contains(PRIVATE));
}
