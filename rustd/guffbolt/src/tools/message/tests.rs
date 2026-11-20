//! `message` over its channel: a line posted, the call's place handed on,
//! and a line the conversation did not take read back as such.

#![expect(
    clippy::expect_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::sync::{Arc, Mutex, PoisonError};

use serde_json::{Value, json};

use super::{LocalChannel, Posted};
use crate::tools::fake::{Asked, Fake};
use crate::tools::outbound::Allowlist;
use crate::tools::{Channel, Local, Refusal, ToolsConfig, code};
use crate::{CallRef, ToolOutput, Tools};

/// Where the call each case makes sits.
fn at() -> CallRef {
    CallRef {
        step: 2,
        index: 0,
        call_id: "call-2-0".to_owned(),
    }
}

/// Calls `message` over `channel` with `arguments`.
async fn said(channel: &dyn Channel, arguments: Value) -> ToolOutput {
    let local = Local::new("run-1", std::env::temp_dir(), Allowlist::Any)
        .expect("the temporary directory exists");
    let tools = Tools::builder()
        .with_channel(channel)
        .build(&ToolsConfig::new(["message"]).expect("a name"))
        .expect("the channel is given");
    let at = at();
    tools
        .dispatch("message", &arguments, local.context(&at))
        .await
}

#[tokio::test]
async fn a_line_is_posted_with_its_call() {
    let fake = Fake::answering(Ok(String::new()));

    let output = said(&fake, json!({"text": "fix pushed as a draft"})).await;

    assert_eq!(
        (output.text.as_str(), output.error_code),
        ("posted to the conversation", None)
    );
    assert_eq!(
        fake.asked(),
        [Asked::Post(at(), "fix pushed as a draft".to_owned())]
    );
}

#[tokio::test]
async fn a_line_the_conversation_did_not_take_is_a_failed_call() {
    let fake = Fake::answering(Ok(String::new())).posting(Posted::NotTaken);

    let output = said(&fake, json!({"text": "status"})).await;

    assert_eq!(output.error_code, Some(code::UPSTREAM_UNREACHABLE));
    assert!(
        output.text.ends_with("say it in the answer instead"),
        "{}",
        output.text
    );
}

#[tokio::test]
async fn a_refused_line_reads_the_channels_reason() {
    let refusal = Refusal::new(code::UPSTREAM_UNREACHABLE, "the thread is closed");
    let fake = Fake::answering(Err(refusal));

    let output = said(&fake, json!({"text": "anyone there?"})).await;

    assert_eq!(output.text, "[upstream_unreachable] the thread is closed");
}

#[tokio::test]
async fn a_message_outside_its_schema_posts_nothing() {
    let fake = Fake::answering(Ok(String::new()));

    for arguments in [
        json!({}),
        json!({"text": 7}),
        json!({"text": "x", "channel": "ops"}),
    ] {
        let output = said(&fake, arguments.clone()).await;
        assert_eq!(
            output.error_code,
            Some(code::INVALID_ARGUMENTS),
            "{arguments}"
        );
    }
    assert_eq!(fake.asked(), []);
}

#[tokio::test]
async fn the_local_channel_hands_each_line_to_its_callback() {
    let heard = Arc::new(Mutex::new(Vec::new()));
    let channel = LocalChannel::new({
        let heard = Arc::clone(&heard);
        move |line: &str| {
            heard
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(line.to_owned());
        }
    });

    let first = said(&channel, json!({"text": "first"})).await;
    let second = said(&channel, json!({"text": "ünïcode line"})).await;

    assert_eq!(first.error_code, None);
    assert_eq!(second.error_code, None);
    let heard = heard.lock().unwrap_or_else(PoisonError::into_inner).clone();
    assert_eq!(heard, ["first", "ünïcode line"]);
}
