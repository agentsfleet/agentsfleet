#![expect(
    clippy::unwrap_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use std::time::Duration;

use bytes::Bytes;
use tokio::sync::mpsc::error::TryRecvError;

use super::Events;
use crate::api::{Ending, ProcessEvent, Stream};
use crate::edges::EDGE_BYTES;

/// Longer than any wait below takes when it works.
const PATIENCE: Duration = Duration::from_secs(5);
/// Long enough for a task that should not finish to show that it has not.
const GLANCE: Duration = Duration::from_millis(50);
/// One kibibyte.
const KIB: usize = 1024;
/// The size of a pipe read on the executor's side.
const CHUNK_BYTES: usize = 16 * KIB;

/// A stdout event of `data`.
fn said(data: &'static [u8]) -> ProcessEvent {
    ProcessEvent::Output {
        stream: Stream::Stdout,
        data: Bytes::from_static(data),
    }
}

#[tokio::test]
async fn output_reads_in_order_then_the_ending_then_nothing() {
    let (feed, mut events) = Events::channel();
    feed.output(Stream::Stdout, Bytes::from_static(b"a"));
    feed.output(Stream::Stdout, Bytes::from_static(b"b"));
    feed.end(Ending::Exited(0));

    assert_eq!(events.recv().await, Some(said(b"a")));
    assert_eq!(events.recv().await, Some(said(b"b")));
    assert_eq!(
        events.recv().await,
        Some(ProcessEvent::Ended {
            ending: Ending::Exited(0)
        })
    );
    assert_eq!(events.recv().await, None);
}

#[tokio::test]
async fn a_feed_gone_without_an_ending_reads_its_output_then_nothing() {
    let (feed, mut events) = Events::channel();
    feed.output(Stream::Stderr, Bytes::from_static(b"x"));
    drop(feed);

    assert!(events.is_finished());
    assert!(matches!(
        events.recv().await,
        Some(ProcessEvent::Output {
            stream: Stream::Stderr,
            ..
        })
    ));
    assert_eq!(events.try_recv(), Err(TryRecvError::Disconnected));
}

#[test]
fn nothing_waiting_reads_empty_while_the_feed_lives() {
    let (feed, mut events) = Events::channel();

    assert_eq!(events.try_recv(), Err(TryRecvError::Empty));
    assert!(!events.is_finished());
    drop(feed);
}

#[tokio::test]
async fn a_waiting_reader_wakes_when_output_arrives() {
    let (feed, mut events) = Events::channel();
    let reader = tokio::spawn(async move { events.recv().await });
    tokio::time::sleep(GLANCE).await;

    feed.output(Stream::Stdout, Bytes::from_static(b"late"));

    let read = tokio::time::timeout(PATIENCE, reader)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(read, Some(said(b"late")));
}

#[tokio::test]
async fn finished_waits_through_output_without_reading_it() {
    let (feed, events) = Events::channel();
    let waiter = tokio::spawn(async move {
        events.finished().await;
        events
    });
    feed.output(Stream::Stdout, Bytes::from_static(b"kept"));
    tokio::time::sleep(GLANCE).await;

    assert!(!waiter.is_finished(), "output alone does not finish it");
    feed.end(Ending::Signaled(9));
    let mut events = tokio::time::timeout(PATIENCE, waiter)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(events.recv().await, Some(said(b"kept")));
}

#[tokio::test]
async fn test_a_caller_that_reads_only_at_the_end_holds_the_edges_and_a_count() {
    let total = 6 * EDGE_BYTES;
    let (feed, mut events) = Events::channel();
    for _ in 0..total / CHUNK_BYTES {
        feed.output(Stream::Stdout, Bytes::from(vec![b'y'; CHUNK_BYTES]));
    }
    feed.end(Ending::Exited(0));
    tokio::time::timeout(PATIENCE, events.finished())
        .await
        .unwrap();

    let (mut kept, mut omitted, mut ending) = (0, 0, None);
    while let Some(event) = events.recv().await {
        match event {
            ProcessEvent::Output { data, .. } => kept += data.len(),
            ProcessEvent::Omitted { bytes } => omitted += bytes,
            ProcessEvent::Ended { ending: ended } => ending = Some(ended),
        }
    }

    assert_eq!(kept, 2 * EDGE_BYTES);
    assert_eq!(omitted, u64::try_from(total - 2 * EDGE_BYTES).unwrap());
    assert_eq!(ending, Some(Ending::Exited(0)));
}
