//! Browser-visible liveness over the real Axum response body, without datastores.
#![expect(clippy::expect_used, reason = "test preconditions must fail loudly")]

use std::time::Duration;

use afd_sse::{Ceiling, Frame};
use axum::body::BodyDataStream;
use futures_util::{FutureExt as _, StreamExt as _, stream};

use super::serve;

const HEARTBEAT_WIRE: &str = "event: heartbeat\ndata: {\"kind\":\"heartbeat\"}\n\n";
const CADENCE: Duration = Duration::from_secs(15);
const BEFORE_CADENCE: Duration = Duration::from_millis(14_999);
const ACTIVITY: &str = "{\"kind\":\"chunk\",\"text\":\"live\"}";
const CONCURRENT_STREAMS: usize = 100;
const ADMISSION_PRECONDITION: &str = "stream admitted";

fn quiet_body(ceiling: &Ceiling) -> BodyDataStream {
    serve(
        stream::pending().boxed(),
        ceiling.admit().expect(ADMISSION_PRECONDITION),
    )
    .into_body()
    .into_data_stream()
}

async fn next_text(body: &mut BodyDataStream) -> String {
    let bytes = body
        .next()
        .await
        .expect("stream remains open")
        .expect("body is infallible");
    String::from_utf8(bytes.to_vec()).expect("SSE is UTF-8")
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
            .send(Frame::activity(sequence, ACTIVITY.into()))
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
