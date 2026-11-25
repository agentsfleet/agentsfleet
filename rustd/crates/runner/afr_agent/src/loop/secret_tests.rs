//! No secret value reaches the model, whether a person, a tool, the model or
//! the fleet's pinned memory wrote it.

use afr_tools::catalog::UPDATE_PLAN;
use tokio_util::sync::CancellationToken;

use super::tests::{drive_with_memory, engine};
use crate::fixture::{API_KEY, Canned, GITHUB_TOKEN, Script, call, payload, say, unbounded};

#[tokio::test]
async fn no_secret_value_is_sent_to_the_model_whoever_wrote_it() {
    let script = Script::new([
        vec![
            say(API_KEY),
            call(
                "c",
                UPDATE_PLAN.name(),
                serde_json::json!({"token": GITHUB_TOKEN}),
            ),
        ],
        vec![say("done")],
    ]);
    let engine = engine(vec![Canned::boxed(&UPDATE_PLAN, GITHUB_TOKEN)], &script);
    let mut held = payload(&[UPDATE_PLAN.name()], unbounded());
    held.event.request_json = format!("{{\"message\":\"use {GITHUB_TOKEN}\"}}").into();
    let pinned = [af_wire::memory::MemoryDelta {
        key: "provider".into(),
        content: format!("the key is {API_KEY}").into(),
        category: af_wire::memory::PINNED_CATEGORY.into(),
        visibility: af_wire::memory::Visibility::Fleet,
    }];
    let memory = afr_memory::Seed::window(&pinned);

    let (_output, _frames) =
        drive_with_memory(&engine, &held, memory, &CancellationToken::new()).await;

    let sent = format!("{:?}", script.sent());
    assert!(
        !sent.contains(API_KEY),
        "the key never reaches a prompt: {sent}"
    );
    assert!(!sent.contains(GITHUB_TOKEN), "nor a credential: {sent}");
    assert!(sent.contains("«secret:github.token»") && sent.contains("«secret:llm.api_key»"));
}
