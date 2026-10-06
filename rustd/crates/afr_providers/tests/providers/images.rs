//! An image a tool read rides the next turn inside the call's result, on the
//! wires that take one; on the chat wire the call is refused before any read.

use std::sync::Arc;
use std::time::Duration;

use afd_wire::report::ResultOutcome;
use afd_wire::tool_trace::ToolCallStatus;
use afr_agent::Loop;
use afr_egress::testing::RecordingTransport;
use afr_tools::Catalog;
use afr_tools::catalog::IMAGE;
use serde_json::json;

use super::support::wires::Wire;
use super::support::{Fake, engine_hosting, lease, run_with};
use super::{ANSWER, CALL_ID};

/// The screenshot the image suite's workspace holds, forty kibibytes of PNG.
const SHOT: &str = "shot.png";
const SHOT_BYTES: usize = 40 * 1024;
/// How long the suite waits on the executor before failing.
const PATIENCE: Duration = Duration::from_secs(20);

/// A real executor served over a scratch workspace holding [`SHOT`].
async fn workspace_with_shot() -> (tempfile::TempDir, afr_executor::Client) {
    let scratch = tempfile::tempdir().unwrap();
    let socket = scratch.path().join("executor.sock");
    let root = scratch.path().join("workspace");
    std::fs::create_dir(&root).unwrap();
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    png.resize(SHOT_BYTES, 0);
    std::fs::write(root.join(SHOT), png).unwrap();
    tokio::spawn(async move { afr_executor::serve(&socket, &root).await });
    let client =
        afr_executor::Client::connect_within(&scratch.path().join("executor.sock"), PATIENCE)
            .await
            .unwrap();
    (scratch, client)
}

/// The loop hosting the runner's whole catalog, driven by `fake`.
fn hosting(fake: &Fake) -> Loop {
    let (transport, _sent) = RecordingTransport::replying(200, "");
    engine_hosting(fake, Catalog::hosted(Arc::new(transport)))
}

#[tokio::test]
async fn test_image_attaches_to_next_turn() {
    let (_scratch, client) = workspace_with_shot().await;
    for wire in [Wire::Messages, Wire::Responses] {
        let mut fake = Fake::serve(vec![
            wire.call(CALL_ID, IMAGE.name(), &json!({"path": SHOT})),
            wire.answer(ANSWER),
        ])
        .await;
        let leased = lease(&wire.provider(), &[IMAGE.name()], "look at the screenshot");

        let (output, _frames) = run_with(&hosting(&fake), &leased, Some(&client)).await;

        assert!(
            matches!(output.result.outcome, ResultOutcome::Completed(_)),
            "{wire:?}: {:?}",
            output.result.outcome
        );
        let trace = output.trace.unwrap();
        assert_eq!(trace.calls[0].status, ToolCallStatus::Succeeded, "{wire:?}");
        let head = trace.calls[0].output_head.as_deref().unwrap_or_default();
        assert!(
            head.starts_with("Attached shot.png (40960 bytes, image/png)"),
            "{wire:?}: the thread reads the path and the size, never the bytes: {head:?}"
        );
        let seen = fake.seen();
        assert_eq!(seen.len(), 2, "{wire:?}");
        assert_eq!(
            wire.images(&seen[1].body),
            ["image/png"],
            "{wire:?}: the next request carries the image inside the call's result"
        );
        assert!(
            wire.images(&seen[0].body).is_empty(),
            "{wire:?}: the first request carried none"
        );
    }
}

/// Chat completions take no image with a tool result, so the call is refused
/// before any read and its result goes back as text alone.
#[tokio::test]
async fn an_image_call_on_the_chat_wire_is_refused_before_any_read() {
    let (_scratch, client) = workspace_with_shot().await;
    let wire = Wire::Chat;
    let mut fake = Fake::serve(vec![
        wire.call(CALL_ID, IMAGE.name(), &json!({"path": SHOT})),
        wire.answer(ANSWER),
    ])
    .await;
    let leased = lease(&wire.provider(), &[IMAGE.name()], "look at the screenshot");

    let (output, _frames) = run_with(&hosting(&fake), &leased, Some(&client)).await;

    let trace = output.trace.unwrap();
    assert_eq!(trace.calls[0].status, ToolCallStatus::Failed);
    let seen = fake.seen();
    let results = wire.results(&seen[1].body);
    assert!(
        results[0].starts_with("[image_input_unavailable] "),
        "the refusal goes back as the call's text: {results:?}"
    );
    assert!(wire.images(&seen[1].body).is_empty());
}
