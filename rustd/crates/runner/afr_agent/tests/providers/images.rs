//! An image a tool read rides the next turn inside the call's result, on the
//! wires that take one. A person's picture arrives with their own words.

use std::sync::Arc;
use std::time::Duration;

use af_egress::testing::RecordingTransport;
use af_wire::tool_trace::ToolCallStatus;
use afr_agent::Loop;
use afr_agent::ResultOutcome;
use afr_tools::Catalog;
use afr_tools::catalog::IMAGE;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::json;

use super::support::wires::Wire;
use super::support::{Fake, engine_hosting, payload, run_with};
use super::{ANSWER, CALL_ID};

/// The screenshot the image suite's workspace holds, forty kibibytes of PNG.
const SHOT: &str = "shot.png";
const SHOT_BYTES: usize = 40 * 1024;
/// How long the suite waits on the executor before failing.
const PATIENCE: Duration = Duration::from_secs(20);

/// A real `format` image, 64 by 64, padded with zeros past its end marker to
/// `length` when that is longer: decoders stop at the marker.
pub(crate) fn real_image(format: image::ImageFormat, length: usize) -> Vec<u8> {
    let picture = image::RgbImage::from_pixel(64, 64, image::Rgb([200, 80, 40]));
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(picture)
        .write_to(&mut bytes, format)
        .unwrap();
    let mut bytes = bytes.into_inner();
    if format == image::ImageFormat::Png && length > bytes.len() {
        bytes.resize(length, 0);
    }
    bytes
}

/// A real executor served over a scratch workspace holding [`SHOT`].
async fn workspace_with_shot() -> (tempfile::TempDir, afr_executor::Client) {
    let scratch = tempfile::tempdir().unwrap();
    let socket = scratch.path().join("executor.sock");
    let root = scratch.path().join("workspace");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(
        root.join(SHOT),
        real_image(image::ImageFormat::Png, SHOT_BYTES),
    )
    .unwrap();
    tokio::spawn(async move { afr_executor::serve(&socket, &root).await });
    let client = afr_executor::Client::connect_within(
        &scratch.path().join("executor.sock"),
        PATIENCE,
        afr_executor::Sizes::DEFAULT,
    )
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
        let assigned = payload(&wire.provider(), &[IMAGE.name()], "look at the screenshot");

        let (output, _frames) = run_with(&hosting(&fake), &assigned, Some(&client)).await;

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

/// The runner withholds image tools from text-only models.
#[tokio::test]
async fn the_chat_wire_is_not_offered_the_image_tool() {
    let wire = Wire::Chat;
    let mut fake = Fake::serve(vec![wire.answer(ANSWER)]).await;
    let assigned = payload(&wire.provider(), &[IMAGE.name()], "look at the screenshot");
    let (output, _frames) = run_with(&hosting(&fake), &assigned, None).await;
    assert!(matches!(output.result.outcome, ResultOutcome::Completed(_)));
    let seen = fake.seen();
    assert!(!seen[0].body.to_string().contains("\"name\":\"image\""));
}

#[tokio::test]
async fn a_message_with_an_image_reaches_the_model() {
    let wire = Wire::Messages;
    let mut fake = Fake::serve(vec![wire.answer(ANSWER)]).await;
    let mut assigned = payload(&wire.provider(), &[], "What colour is the attached image?");
    let data = STANDARD.encode(real_image(image::ImageFormat::Png, 0));
    assigned.event.request_json = json!({
        "message": "What colour is the attached image?",
        "images": [{"media_type": "image/png", "data": data}],
    })
    .to_string()
    .into();
    let (output, _frames) = run_with(&hosting(&fake), &assigned, None).await;
    assert!(matches!(output.result.outcome, ResultOutcome::Completed(_)));
    let seen = fake.seen();
    let message = seen[0].body["messages"].as_array().unwrap().last().unwrap();
    assert_eq!(message["role"], "user");
    assert_eq!(
        message["content"][0]["text"],
        "What colour is the attached image?"
    );
    assert_eq!(message["content"][1]["type"], "image");
    assert_eq!(message["content"][1]["source"]["media_type"], "image/png");
    assert_eq!(message["content"][1]["source"]["data"], data);
}
