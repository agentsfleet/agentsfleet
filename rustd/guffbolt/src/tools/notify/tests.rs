//! `notify` over its notifier: the notification as the model wrote it, the
//! call's place handed on, and the vendor's answer or refusal read back.

#![expect(
    clippy::expect_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use serde_json::{Value, json};

use super::Priority;
use crate::tools::fake::{Asked, Fake};
use crate::tools::outbound::Allowlist;
use crate::tools::{Local, Refusal, ToolsConfig, code};
use crate::{CallRef, ToolOutput, Tools};

/// Where the call each case makes sits.
fn at() -> CallRef {
    CallRef {
        step: 4,
        index: 2,
        call_id: "call-4-2".to_owned(),
    }
}

/// Calls `notify` over `fake` with `arguments`.
async fn notified(fake: &Fake, arguments: Value) -> ToolOutput {
    let local = Local::new("run-1", std::env::temp_dir(), Allowlist::Any)
        .expect("the temporary directory exists");
    let tools = Tools::builder()
        .with_notifier(fake)
        .build(&ToolsConfig::new(["notify"]).expect("a name"))
        .expect("the notifier is given");
    let at = at();
    tools
        .dispatch("notify", &arguments, local.context(&at))
        .await
}

#[tokio::test]
async fn a_notification_reaches_the_notifier_as_written() {
    let fake = Fake::answering(Ok("sent".to_owned()));

    let titled = notified(
        &fake,
        json!({"message": "deploy finished", "title": "Prod", "priority": "high"}),
    )
    .await;
    let plain = notified(&fake, json!({"message": "nightly done"})).await;

    assert_eq!((titled.text.as_str(), titled.error_code), ("sent", None));
    assert_eq!(plain.error_code, None);
    assert_eq!(
        fake.asked(),
        [
            Asked::Notify(
                at(),
                "deploy finished".to_owned(),
                Some("Prod".to_owned()),
                Priority::High
            ),
            Asked::Notify(at(), "nightly done".to_owned(), None, Priority::Normal),
        ]
    );
}

#[tokio::test]
async fn a_vendor_refusal_reads_its_code() {
    let fake = Fake::answering(Err(Refusal::new(code::UPSTREAM_STATUS, "Status: 429")));

    let output = notified(&fake, json!({"message": "x"})).await;

    assert_eq!(output.text, "[upstream_status] Status: 429");
}

#[tokio::test]
async fn a_credential_or_a_vendor_word_in_the_arguments_sends_nothing() {
    let fake = Fake::answering(Ok("sent".to_owned()));

    for arguments in [
        json!({}),
        json!({"message": "x", "token": "model_supplied"}),
        json!({"message": "x", "priority": 2}),
        json!({"message": "x", "priority": "emergency"}),
        json!({"message": "x", "priority": "High"}),
        json!({"message": "x", "sound": "siren"}),
    ] {
        let output = notified(&fake, arguments.clone()).await;
        assert_eq!(
            output.error_code,
            Some(code::INVALID_ARGUMENTS),
            "{arguments}"
        );
    }
    assert_eq!(fake.asked(), []);
}
