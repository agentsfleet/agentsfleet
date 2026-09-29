//! Browser-visible liveness and wire bytes over the real response body,
//! without datastores.
#![expect(clippy::expect_used, reason = "test preconditions must fail loudly")]

use std::collections::BTreeMap;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use afd_dragonfly::Message;
use afd_sse::{Ceiling, Frame};
use axum::body::{BodyDataStream, Bytes};
use axum::response::IntoResponse as _;
use axum::response::sse::{Event, Sse};
use futures_util::{FutureExt as _, StreamExt as _, stream};

use super::serve;

const HEARTBEAT_WIRE: &str = "event: heartbeat\ndata: {\"kind\":\"heartbeat\"}\n\n";
const CADENCE: Duration = Duration::from_secs(15);
const BEFORE_CADENCE: Duration = Duration::from_millis(14_999);
const ACTIVITY: &str = "{\"kind\":\"chunk\",\"text\":\"live\"}";
const CONCURRENT_STREAMS: usize = 100;
const ADMISSION_PRECONDITION: &str = "stream admitted";
const FLEET: &str = "01924f4e-0000-7000-8000-00000000fee7";

/// `payload` as the hub hands it to every reader of its channel.
fn published(payload: &str) -> Arc<Message> {
    Arc::new(Message {
        channel: format!("fleet:{FLEET}:activity"),
        payload: payload.to_owned(),
    })
}

fn quiet_body(ceiling: &Ceiling) -> BodyDataStream {
    serve(
        stream::pending().boxed(),
        ceiling.admit().expect(ADMISSION_PRECONDITION),
    )
    .into_body()
    .into_data_stream()
}

/// The chunks of one event, read up to the blank line that ends it. A frame
/// is written in pieces so its payload can be shared; a heartbeat is one.
async fn next_chunks(body: &mut BodyDataStream) -> Vec<Bytes> {
    let mut chunks: Vec<Bytes> = Vec::new();
    while !chunks.last().is_some_and(|chunk| chunk.ends_with(b"\n\n")) {
        let chunk = body
            .next()
            .await
            .expect("stream remains open")
            .expect("body is infallible");
        chunks.push(chunk);
    }
    chunks
}

async fn next_text(body: &mut BodyDataStream) -> String {
    let bytes: Vec<u8> = next_chunks(body).await.concat();
    String::from_utf8(bytes).expect("SSE is UTF-8")
}

#[tokio::test(start_paused = true)]
async fn an_idle_stream_emits_named_heartbeats_at_the_documented_cadence() {
    let ceiling = Ceiling::new(1);
    let mut body = quiet_body(&ceiling);
    for _ in 0..3 {
        assert!(body.next().now_or_never().is_none());
        tokio::time::advance(BEFORE_CADENCE).await;
        assert!(body.next().now_or_never().is_none());
        tokio::time::advance(Duration::from_millis(1)).await;
        assert_eq!(next_text(&mut body).await, HEARTBEAT_WIRE);
        assert_eq!(ceiling.live(), 1);
    }
    drop(body);
    assert_eq!(ceiling.live(), 0);
}

#[tokio::test(start_paused = true)]
async fn heartbeats_neither_consume_activity_ids_nor_delay_ready_activity() {
    let ceiling = Ceiling::new(1);
    let (sender, receiver) = tokio::sync::mpsc::channel(1);
    let frames = stream::unfold(receiver, |mut receiver| async move {
        receiver.recv().await.map(|frame| (frame, receiver))
    });
    let mut body = serve(
        frames.boxed(),
        ceiling.admit().expect(ADMISSION_PRECONDITION),
    )
    .into_body()
    .into_data_stream();
    for sequence in [0, 1] {
        tokio::time::advance(CADENCE).await;
        assert_eq!(next_text(&mut body).await, HEARTBEAT_WIRE);
        sender
            .send(Frame::activity(sequence, published(ACTIVITY)))
            .await
            .expect("reader alive");
        let frame = next_text(&mut body).await;
        assert_eq!(
            frame,
            format!("id: {sequence}\nevent: chunk\ndata: {ACTIVITY}\n\n")
        );
        tokio::time::advance(BEFORE_CADENCE).await;
        assert!(body.next().now_or_never().is_none());
    }
    drop(sender);
    assert!(
        body.next().await.is_none(),
        "closed activity ends the heartbeat too"
    );
    assert_eq!(ceiling.live(), 0);
}

#[tokio::test(start_paused = true)]
async fn one_hundred_idle_streams_keep_their_slots_until_each_body_drops() {
    let ceiling = Ceiling::new(CONCURRENT_STREAMS);
    let mut bodies: Vec<_> = (0..CONCURRENT_STREAMS)
        .map(|_| quiet_body(&ceiling))
        .collect();
    assert_eq!(ceiling.live(), CONCURRENT_STREAMS);
    assert!(ceiling.admit().is_none());
    tokio::time::advance(CADENCE).await;
    for body in &mut bodies {
        assert_eq!(next_text(body).await, HEARTBEAT_WIRE);
    }
    for remaining in (0..CONCURRENT_STREAMS).rev() {
        drop(bodies.pop());
        assert_eq!(ceiling.live(), remaining);
    }
    assert!(ceiling.admit().is_some(), "all capacity can be reused");
}

#[tokio::test(start_paused = true)]
async fn dropping_an_unpolled_response_releases_its_slot() {
    let ceiling = Ceiling::new(1);
    let body = quiet_body(&ceiling);
    assert_eq!(ceiling.live(), 1);
    drop(body);
    assert_eq!(ceiling.live(), 0);
}

#[tokio::test(start_paused = true)]
async fn a_finished_stream_does_not_emit_a_posthumous_heartbeat() {
    let ceiling = Ceiling::new(1);
    let mut body = serve(
        stream::empty().boxed(),
        ceiling.admit().expect(ADMISSION_PRECONDITION),
    )
    .into_body()
    .into_data_stream();
    tokio::time::advance(CADENCE).await;
    assert!(body.next().await.is_none());
    assert_eq!(ceiling.live(), 0);
}

/// Every frame shape reaches the wire as the bytes `axum`'s `Sse` wrote for
/// it — the field order, the empty-data case, and a line break continued as
/// another `data:` line — so replacing it changed what is shared, not what a
/// browser reads.
#[tokio::test]
async fn the_body_writes_what_axum_would() {
    let frames = vec![
        Frame::activity(0, published(ACTIVITY)),
        Frame::activity(
            1,
            published("{\"kind\":\"chunk\",\"text\":\"a\nb\rc\r\n\"}"),
        ),
        Frame::activity(2, published("")),
        Frame::tagged(3, FLEET, published(ACTIVITY)).expect("an object takes a tag"),
        Frame::tagged(4, FLEET, published("{}")).expect("an empty object takes a tag"),
        Frame::hello(&[FLEET.to_owned()], &BTreeMap::new()),
        Frame::catching_up(0),
    ];
    let events: Vec<Result<Event, Infallible>> = frames
        .iter()
        .map(|frame| {
            Ok(Event::default()
                .id(frame.seq.to_string())
                .event(frame.kind.as_ref())
                .data(frame.data.text()))
        })
        .collect();
    let ceiling = Ceiling::new(1);
    let ours = serve(
        stream::iter(frames).boxed(),
        ceiling.admit().expect(ADMISSION_PRECONDITION),
    )
    .into_body();
    let axums = Sse::new(stream::iter(events)).into_response().into_body();

    let ours = axum::body::to_bytes(ours, usize::MAX)
        .await
        .expect("infallible");
    let axums = axum::body::to_bytes(axums, usize::MAX)
        .await
        .expect("infallible");
    assert_eq!(
        String::from_utf8_lossy(&ours),
        String::from_utf8_lossy(&axums)
    );
}

/// Dimension 5.4, the body's half: three viewers of one frame write its
/// payload from the one allocation the hub dispatched.
#[tokio::test]
async fn test_the_body_shares_one_payload_across_viewers() {
    let message = published(ACTIVITY);
    let ceiling = Ceiling::new(3);
    for viewer in 0..3 {
        let frame = Frame::activity(viewer, Arc::clone(&message));
        let mut body = serve(
            stream::iter([frame]).boxed(),
            ceiling.admit().expect(ADMISSION_PRECONDITION),
        )
        .into_body()
        .into_data_stream();
        let [prefix, payload, end] = <[Bytes; 3]>::try_from(next_chunks(&mut body).await)
            .expect("a shared frame is prefix, payload and terminator");
        assert_eq!(
            prefix.as_ref(),
            format!("id: {viewer}\nevent: chunk\ndata: ").as_bytes()
        );
        assert_eq!(
            payload.as_ptr(),
            message.payload.as_ptr(),
            "viewer {viewer} wrote the published bytes, not a copy of them"
        );
        assert_eq!(end.as_ref(), b"\n\n");
    }
}
