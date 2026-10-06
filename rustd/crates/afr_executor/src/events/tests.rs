#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use std::time::Duration;

use bytes::Bytes;
use tokio::sync::mpsc::error::TryRecvError;

use super::Events;
use crate::api::{Ending, Process, ProcessEvent, ProcessId, Stream};
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
    feed.end(Ending::Exited(0), false);

    assert_eq!(events.recv().await, Some(said(b"a")));
    assert_eq!(events.recv().await, Some(said(b"b")));
    assert_eq!(
        events.recv().await,
        Some(ProcessEvent::Ended {
            ending: Ending::Exited(0),
            output_abandoned: false
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
    feed.end(Ending::Signaled(9), false);
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
    feed.end(Ending::Exited(0), false);
    tokio::time::timeout(PATIENCE, events.finished())
        .await
        .unwrap();

    let (mut kept, mut omitted, mut ending) = (0, 0, None);
    while let Some(event) = events.recv().await {
        match event {
            ProcessEvent::Output { data, .. } => kept += data.len(),
            ProcessEvent::Omitted { bytes } => omitted += bytes,
            ProcessEvent::Ended { ending: ended, .. } => ending = Some(ended),
        }
    }

    assert_eq!(kept, 2 * EDGE_BYTES);
    assert_eq!(omitted, u64::try_from(total - 2 * EDGE_BYTES).unwrap());
    assert_eq!(ending, Some(Ending::Exited(0)));
}

/// A feed on another thread wakes a reader that is waiting, every time: the
/// reader ends with every byte it was fed either read or counted, and the
/// ending, never stuck on a wakeup that came between its look and its wait.
#[tokio::test]
async fn a_feed_on_another_thread_loses_no_wakeup_and_no_byte() {
    const PIECES: usize = 20_000;
    const PIECE: &[u8] = b"0123456789abcdef";
    let (feed, mut events) = Events::channel();
    let feeding = std::thread::spawn(move || {
        for _ in 0..PIECES {
            feed.output(Stream::Stdout, Bytes::from_static(PIECE));
        }
        feed.end(Ending::Exited(0), false);
    });

    let (read, omitted, ending) = tokio::time::timeout(PATIENCE, async {
        let (mut read, mut omitted) = (0, 0);
        loop {
            match events.recv().await {
                Some(ProcessEvent::Output { data, .. }) => read += data.len(),
                Some(ProcessEvent::Omitted { bytes }) => omitted += bytes,
                Some(ProcessEvent::Ended { ending, .. }) => return (read, omitted, ending),
                None => panic!("the events finished with no ending"),
            }
        }
    })
    .await
    .unwrap();
    feeding.join().unwrap();

    assert_eq!(ending, Ending::Exited(0));
    assert_eq!(
        read as u64 + omitted,
        (PIECES * PIECE.len()) as u64,
        "every byte read or counted"
    );
}

/// A caller reading a process to its end hands each chunk on and passes over
/// a gap: it is told the ending, never the count, and holds only the edges.
#[tokio::test]
async fn a_process_read_to_its_end_passes_over_what_was_dropped() {
    let (feed, events) = Events::channel();
    for _ in 0..(6 * EDGE_BYTES) / CHUNK_BYTES {
        feed.output(Stream::Stdout, Bytes::from(vec![b'y'; CHUNK_BYTES]));
    }
    feed.end(Ending::Exited(3), false);
    let process = Process {
        id: ProcessId::new(1),
        events,
    };

    let mut handed = 0;
    let ending = process.ended(|_stream, data| handed += data.len()).await;

    assert_eq!(ending, Some(Ending::Exited(3)));
    assert_eq!(handed, 2 * EDGE_BYTES);
}

/// Events that finish with no ending, as a lost connection's do, read to
/// their end as none, after the output they carried.
#[tokio::test]
async fn a_process_whose_events_finish_with_no_ending_reads_to_none() {
    let (feed, events) = Events::channel();
    feed.output(Stream::Stdout, Bytes::from_static(b"partial"));
    drop(feed);
    let process = Process {
        id: ProcessId::new(2),
        events,
    };

    let mut handed = Vec::new();
    let ending = process
        .ended(|_stream, data| handed.extend_from_slice(&data))
        .await;

    assert_eq!(ending, None);
    assert_eq!(handed, b"partial");
}

#[tokio::test]
async fn an_ending_carries_whether_output_was_left_behind() {
    let (feed, mut events) = Events::channel();
    feed.end(Ending::Exited(0), true);

    assert_eq!(
        events.recv().await,
        Some(ProcessEvent::Ended {
            ending: Ending::Exited(0),
            output_abandoned: true,
        })
    );
}
