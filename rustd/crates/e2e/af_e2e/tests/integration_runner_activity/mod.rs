//! Native activity publishes through the Bus only after decoding and run ownership checks.
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "integration preconditions fail loudly"
)]

mod fixture;
pub(crate) use fixture::Tailed;

use agentsfleetd::supervisor::Supervisor;
use serde_json::json;

use crate::wire::field;

pub(crate) const TOOL_NAME: &str = "shell";
const CHUNK_TEXT: &str = "the fleet said this out loud";
const UNHELD_RUN: &str = "0195b4ba-8d3a-7fff-8abc-ffffffffffff";

/// A valid batch publishes; malformed and unowned activity have no effect.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_activity_publish() {
    let mut supervisor = Supervisor::new();
    let mut tailed = Tailed::open(&mut supervisor).await;
    tailed
        .forward(&json!({"frames": [{"fleet_response_chunk": {
            "text": CHUNK_TEXT, "text_kind": "answer"
        }}]}))
        .await;
    let frame = tailed
        .next_frame()
        .await
        .expect("the frame reaches the tail");
    assert_eq!(field(&frame, "kind"), &json!("chunk"));
    assert_eq!(field(&frame, "text"), &json!(CHUNK_TEXT));
    assert_eq!(
        field(&frame, "turn_id"),
        &json!(tailed.scene.turn_id),
        "activity is grouped by the event, never the run"
    );
    tailed = tailed
        .refused(&json!({"frames": [{"no_such_frame": {"text": "?"}}]}))
        .await;

    tailed.finish(supervisor).await;
}

/// A rendering failure drops that frame while a later valid frame still publishes.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_activity_drops_a_frame_it_cannot_render() {
    let mut supervisor = Supervisor::new();
    let mut tailed = Tailed::open(&mut supervisor).await;
    tailed
        .forward(&json!({"frames": [{"tool_call_started": {
            "name":TOOL_NAME, "args_redacted":"this is not JSON", "call_id":"1"
        }}]}))
        .await;
    assert_eq!(
        tailed.silence().await,
        None,
        "unrenderable arguments do not publish"
    );
    tailed
        .forward(&json!({"frames": [{"fleet_response_chunk": {
            "text":CHUNK_TEXT, "text_kind":"answer"
        }}]}))
        .await;
    let frame = tailed
        .next_frame()
        .await
        .expect("one dropped frame does not poison the run");
    assert_eq!(field(&frame, "text"), &json!(CHUNK_TEXT));
    assert_eq!(
        tailed.silence().await,
        None,
        "only the renderable frame publishes"
    );
    tailed.finish(supervisor).await;
}

/// An unowned frame cannot publish, and the same connection still serves its real run.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn an_unowned_native_activity_frame_reaches_no_tail() {
    let mut supervisor = Supervisor::new();
    let mut tailed = Tailed::open(&mut supervisor).await;
    tailed
        .forward_for(
            UNHELD_RUN,
            &json!({"frames": [{"fleet_response_chunk": {
                "text":"an unowned run must not publish", "text_kind":"answer"
            }}]}),
        )
        .await;
    assert_eq!(
        tailed.silence().await,
        None,
        "an unowned run has no cached target"
    );
    tailed
        .forward(&json!({"frames": [{"fleet_response_chunk": {
            "text":CHUNK_TEXT, "text_kind":"answer"
        }}]}))
        .await;
    let frame = tailed
        .next_frame()
        .await
        .expect("the native session remains usable");
    assert_eq!(field(&frame, "text"), &json!(CHUNK_TEXT));
    assert_eq!(
        tailed.silence().await,
        None,
        "no unowned frame arrived late"
    );
    tailed.finish(supervisor).await;
}

/// The pieces a streamed answer arrives in.
const DELTAS: [&str; 3] = ["Deployed ", "to ", "staging."];

/// A streamed answer's deltas reach the turn's stream in order, and none is
/// kept: the turn's only item is the tool call the run recorded.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn text_deltas_fan_out_and_are_not_stored() {
    let mut supervisor = Supervisor::new();
    let mut tailed = Tailed::open(&mut supervisor).await;
    let http = afd_identity::direct(std::time::Duration::from_secs(5))
        .build()
        .expect("a direct client builds");
    let (status, body) = crate::integration_tool_call_details::post_records(
        &http,
        &tailed.scene,
        (&tailed.run, tailed.fence),
        &[crate::integration_tool_call_details::record(1, "ok")],
    )
    .await;
    assert_eq!(status, 200, "{body}");
    let frames: Vec<_> = DELTAS
        .iter()
        .map(|text| json!({"fleet_response_chunk": {"text": text, "text_kind": "answer"}}))
        .collect();
    tailed.forward(&json!({ "frames": frames })).await;
    for delta in DELTAS {
        let frame = tailed
            .next_frame()
            .await
            .expect("each delta reaches the stream");
        assert_eq!(field(&frame, "kind"), &json!("chunk"));
        assert_eq!(field(&frame, "text"), &json!(delta));
        assert_eq!(field(&frame, "turn_id"), &json!(tailed.scene.turn_id));
    }
    assert_eq!(tailed.silence().await, None, "only the deltas were sent");
    let mut connection = tailed
        .scene
        .booted
        .database
        .acquire()
        .await
        .expect("a connection");
    let kept: Vec<(i64, String)> = sqlx::query_as(
        "SELECT sequence, output FROM thread.items WHERE turn_id = $1 ORDER BY sequence",
    )
    .bind(&tailed.scene.turn_id)
    .fetch_all(&mut *connection)
    .await
    .expect("the turn's items");
    drop(connection);
    assert_eq!(
        kept,
        vec![(1, "ok".to_owned())],
        "no delta is stored as an item"
    );
    tailed.finish(supervisor).await;
}
