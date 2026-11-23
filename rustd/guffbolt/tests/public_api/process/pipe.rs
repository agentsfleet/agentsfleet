//! The event pipe an environment feeds.

use super::*;

/// Every event a reader takes from `events` as it stands.
fn drained(events: &mut guffbolt::tools::env::Piped) -> (Vec<ProcessEvent>, bool) {
    events.take_ready()
}

fn output(data: Vec<u8>) -> ProcessEvent {
    ProcessEvent::Output {
        stream: Stream::Stdout,
        data: Bytes::from(data),
    }
}

/// A chunk that crosses the head's edge is cut there, on a character
/// boundary, or where the cap falls for bytes that are not text; nothing is
/// lost while the tail holds it.
#[test]
fn a_chunk_across_the_heads_edge_is_cut_there_and_read_whole() {
    let (feed, mut events) = pipe();
    let mut chunk = vec![b'a'; EDGE_BYTES - 1];
    chunk.push(0xFF);
    chunk.extend_from_slice(b"zzzzz");

    feed.output(Stream::Stdout, Bytes::from(chunk));
    feed.output(Stream::Stdout, Bytes::new());
    feed.end(Ending::Exited(0), false);
    let (read, finished) = drained(&mut events);

    let mut head = vec![b'a'; EDGE_BYTES - 1];
    head.push(0xFF);
    assert_eq!(
        read,
        [
            output(head),
            output(b"zzzzz".to_vec()),
            ProcessEvent::Ended {
                ending: Ending::Exited(0),
                output_abandoned: false
            },
        ],
        "the empty chunk was never kept"
    );
    assert!(finished);
}

/// A character the head's last chunk started, whose rest fell into the gap,
/// joins the gap rather than reading as noise before it.
#[test]
fn a_character_cut_by_the_gap_joins_the_gap() {
    let (feed, mut events) = pipe();
    feed.output(Stream::Stdout, Bytes::from(vec![b'a'; EDGE_BYTES - 2]));
    feed.output(Stream::Stdout, Bytes::from_static(&[0xC3]));
    let mut overflow = vec![0xC3, 0xA9];
    overflow.extend(vec![b'b'; EDGE_BYTES]);
    feed.output(Stream::Stdout, Bytes::from(overflow));
    drop(feed);

    let (read, finished) = drained(&mut events);

    assert_eq!(
        read,
        [
            output(vec![b'a'; EDGE_BYTES - 2]),
            ProcessEvent::Omitted { bytes: 3 },
            output(vec![b'b'; EDGE_BYTES]),
        ]
    );
    assert!(
        finished,
        "a dropped feed finishes the events with no ending"
    );
}

/// `recv` waits for what the feed says next; `Process::ended` reads output
/// to the ending, passing over a gap, and none when the events finish first.
#[tokio::test]
async fn a_reader_waits_for_the_feed_and_reads_to_the_ending() {
    let (feed, mut events) = pipe();
    let reader = tokio::spawn(async move {
        let first = events.recv().await;
        events.disconnect();
        (first, events)
    });
    tokio::task::yield_now().await;
    feed.output(Stream::Stderr, Bytes::from_static(b"late"));
    let (first, events) = reader.await.expect("the reader ran");
    feed.output(Stream::Stdout, Bytes::from(vec![b'x'; EDGE_BYTES + 10]));
    feed.output(Stream::Stdout, Bytes::from(vec![b'y'; EDGE_BYTES]));
    feed.end(Ending::Signaled(15), true);
    let mut heard = Vec::new();

    let ending = Process {
        id: ProcessId::new(1),
        events: Box::new(events),
    }
    .ended(|stream, data| heard.push((stream, data.len())))
    .await;
    let (gone, nothing) = pipe();
    drop(gone);
    let none = Process {
        id: ProcessId::new(2),
        events: Box::new(nothing),
    }
    .ended(|_, _| {})
    .await;

    assert_eq!(
        first,
        Some(ProcessEvent::Output {
            stream: Stream::Stderr,
            data: Bytes::from_static(b"late")
        })
    );
    assert_eq!(ending, Some(Ending::Signaled(15)));
    assert_eq!(
        heard,
        [
            (Stream::Stdout, EDGE_BYTES - 4),
            (Stream::Stdout, EDGE_BYTES)
        ],
        "the head (short of what was read before), then the tail; the gap is passed over"
    );
    assert_eq!(none, None);
}
