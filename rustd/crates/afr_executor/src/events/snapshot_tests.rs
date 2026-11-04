//! Later producer writes cannot extend a previously captured read.
#![expect(clippy::expect_used, reason = "test preconditions fail loudly")]

use bytes::Bytes;
use tokio::sync::mpsc::error::TryRecvError;

use super::Events;
use crate::{EDGE_BYTES, Ending, ProcessEvent, Stream};

#[derive(Debug, Default, PartialEq, Eq)]
struct Observed {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    terminal: Vec<u8>,
    omitted: u64,
    ending: Option<(Ending, bool)>,
    events: usize,
}

impl FromIterator<ProcessEvent> for Observed {
    fn from_iter<T: IntoIterator<Item = ProcessEvent>>(iter: T) -> Self {
        let mut observed = Self::default();
        for event in iter {
            observed.events += 1;
            match event {
                ProcessEvent::Output { stream, data } => match stream {
                    Stream::Stdout => observed.stdout.extend_from_slice(&data),
                    Stream::Stderr => observed.stderr.extend_from_slice(&data),
                    Stream::Terminal => observed.terminal.extend_from_slice(&data),
                },
                ProcessEvent::Omitted { bytes } => observed.omitted += bytes,
                ProcessEvent::Ended {
                    ending,
                    output_abandoned,
                } => {
                    assert!(
                        observed
                            .ending
                            .replace((ending, output_abandoned))
                            .is_none()
                    );
                }
            }
        }
        observed
    }
}

// Value: protects=a read stays finite when another thread adds output and an ending;
// fails_when=later channel state is consulted while consuming the captured iterator;
// seam=none; no_claim=kernel process cleanup or socket transport.
#[test]
fn later_output_and_ending_wait_for_the_next_snapshot() {
    let (feed, mut events) = Events::channel();
    feed.output(Stream::Stdout, Bytes::from(vec![b'a'; 3 * EDGE_BYTES]));
    let (mut first, finished) = events.take_ready();
    let head = first.next().expect("initial head");
    let producer = std::thread::spawn(move || {
        feed.output(Stream::Stderr, Bytes::from(vec![b'b'; 3 * EDGE_BYTES]));
        feed.end(Ending::Exited(7), true);
    });
    producer.join().expect("later producer finishes");
    assert!(!finished, "completion belongs to the captured state");
    assert_eq!(
        std::iter::once(head).chain(first).collect::<Observed>(),
        Observed {
            stdout: vec![b'a'; 2 * EDGE_BYTES],
            omitted: u64::try_from(EDGE_BYTES).expect("bounded bytes"),
            ending: None,
            events: 3,
            ..Observed::default()
        }
    );
    let (second, finished) = events.take_ready();
    assert!(finished);
    assert_eq!(
        second.collect::<Observed>(),
        Observed {
            stderr: vec![b'b'; 2 * EDGE_BYTES],
            omitted: u64::try_from(EDGE_BYTES).expect("bounded bytes"),
            ending: Some((Ending::Exited(7), true)),
            events: 4,
            ..Observed::default()
        }
    );
    assert_eq!(events.try_recv(), Err(TryRecvError::Disconnected));
}

// Value: protects=a disconnected channel retains its bytes and lacks a forged ending;
// fails_when=snapshotting discards retained output or invents a successful status;
// seam=none; no_claim=wire error classification.
#[test]
fn disconnect_preserves_buffered_output_without_creating_an_ending() {
    let (feed, mut events) = Events::channel();
    feed.output(Stream::Stdout, Bytes::from_static(b"partial"));
    drop(feed);
    let (snapshot, finished) = events.take_ready();
    assert!(finished);
    assert_eq!(
        snapshot.collect::<Observed>(),
        Observed {
            stdout: b"partial".to_vec(),
            events: 1,
            ..Observed::default()
        }
    );
    let (empty, still_finished) = events.take_ready();
    assert!(still_finished);
    assert_eq!(empty.collect::<Observed>(), Observed::default());
}

// Value: protects=zero-byte frames consume no entries while real output remains readable;
// fails_when=empty frames accumulate or filtering also removes bytes or completion;
// seam=none; no_claim=malformed socket frame parsing.
#[test]
fn empty_frames_do_not_accumulate_or_erase_later_output() {
    let (feed, mut events) = Events::channel();
    events.disconnect(); // A locally fed channel has no native link to close.
    for _ in 0..1_000 {
        feed.output(Stream::Stdout, Bytes::new());
    }
    let (empty, finished) = events.take_ready();
    assert!(!finished);
    assert_eq!(empty.collect::<Observed>(), Observed::default());
    feed.output(Stream::Terminal, Bytes::from_static(b"kept"));
    feed.end(Ending::Exited(0), false);
    let (snapshot, finished) = events.take_ready();
    assert!(finished);
    assert_eq!(
        snapshot.collect::<Observed>(),
        Observed {
            terminal: b"kept".to_vec(),
            ending: Some((Ending::Exited(0), false)),
            events: 2,
            ..Observed::default()
        }
    );
    assert_eq!(events.try_recv(), Err(TryRecvError::Disconnected));
}
