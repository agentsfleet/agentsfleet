#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use bytes::Bytes;

use super::{Chunk, EDGE_BYTES, OutputEdges};
use crate::api::Stream;

/// One kibibyte.
const KIB: usize = 1024;
/// One mebibyte.
const MIB: usize = KIB * KIB;

/// Feeds `input` in `piece`-sized chunks and returns what was emitted live,
/// what the tail held, and the omitted count.
fn run(edge: usize, input: &[u8], piece: usize) -> (Vec<u8>, Vec<u8>, u64) {
    let mut edges = OutputEdges::new(edge);
    let mut head = Vec::new();
    let mut tail = Vec::new();
    for slice in input.chunks(piece) {
        let chunk = Chunk {
            stream: Stream::Stdout,
            data: Bytes::copy_from_slice(slice),
        };
        edges.feed(chunk, &mut |kept| head.extend_from_slice(&kept.data));
    }
    let omitted = edges.finish(&mut |kept| tail.extend_from_slice(&kept.data));
    (head, tail, omitted)
}

#[test]
fn test_executor_output_keeps_head_and_tail() {
    // A three-byte character, so 512 KiB lands inside one on both edges.
    let text = "€".repeat(3 * MIB / 3);
    let (head, tail, omitted) = run(EDGE_BYTES, text.as_bytes(), 64 * KIB);

    assert!(
        std::str::from_utf8(&head).is_ok(),
        "the head is cut on a character boundary"
    );
    assert!(
        std::str::from_utf8(&tail).is_ok(),
        "the tail starts on a character boundary"
    );
    assert!(
        head.len() <= EDGE_BYTES && head.len() > EDGE_BYTES - 3,
        "{}",
        head.len()
    );
    assert!(
        tail.len() <= EDGE_BYTES && tail.len() > EDGE_BYTES - 3,
        "{}",
        tail.len()
    );
    assert_eq!(
        head.len() as u64 + tail.len() as u64 + omitted,
        text.len() as u64,
        "every byte is kept or counted"
    );
}

#[test]
fn output_under_the_head_is_forwarded_whole_and_nothing_is_omitted() {
    let (head, tail, omitted) = run(16, b"short", 2);

    assert_eq!(head, b"short");
    assert!(tail.is_empty());
    assert_eq!(omitted, 0);
}

#[test]
fn output_between_the_edges_keeps_every_byte() {
    let input: Vec<u8> = (0..24).collect();
    let (head, tail, omitted) = run(16, &input, 5);

    assert_eq!(head, &input[..16]);
    assert_eq!(tail, &input[16..]);
    assert_eq!(omitted, 0);
}

#[test]
fn a_chunk_that_starts_inside_a_character_is_never_emitted_empty() {
    // The head has one byte left and the chunk opens with a two-byte
    // character, so the cut moves back to zero and nothing is sent live.
    let mut edges = OutputEdges::new(1);
    let mut live = Vec::new();
    edges.feed(
        Chunk {
            stream: Stream::Terminal,
            data: Bytes::from_static("é!".as_bytes()),
        },
        &mut |chunk| live.push(chunk),
    );
    let mut tail = Vec::new();
    let omitted = edges.finish(&mut |chunk| tail.push(chunk));

    assert!(live.is_empty());
    assert_eq!(tail.len(), 1);
    assert_eq!(tail.first().unwrap().data.as_ref(), "!".as_bytes());
    assert_eq!(tail.first().unwrap().stream, Stream::Terminal);
    assert_eq!(omitted, 2, "the character that no longer fits either edge");
}

#[test]
fn a_tail_chunk_dropped_whole_leaves_the_next_one_first() {
    let (head, tail, omitted) = run(4, b"aaaabbbbccccdddd", 4);

    assert_eq!(head, b"aaaa");
    assert_eq!(tail, b"dddd");
    assert_eq!(omitted, 8);
}

#[test]
fn a_head_filled_by_a_whole_chunk_still_ends_on_a_character() {
    // The first four-byte chunk fills the head exactly and ends two bytes into
    // a euro sign, so the cut moves back to the sign's first byte.
    let input = "aa€b".as_bytes();
    let (head, tail, omitted) = run(4, input, 4);

    assert_eq!(head, b"aa");
    assert_eq!(tail, "€b".as_bytes());
    assert_eq!(omitted, 0);
}

#[test]
fn a_chunk_ending_on_a_complete_character_is_kept_whole_in_the_head() {
    let input = "a€".as_bytes();
    let (head, tail, omitted) = run(4, input, 4);

    assert_eq!(head, input);
    assert!(tail.is_empty());
    assert_eq!(omitted, 0);
}

#[test]
fn binary_output_is_cut_where_the_cap_falls() {
    let input = [0xff_u8; 8];
    let (head, tail, omitted) = run(3, &input, 8);

    assert_eq!(head, [0xff; 3]);
    assert_eq!(tail, [0xff; 3]);
    assert_eq!(omitted, 2);
}

#[test]
fn a_two_byte_and_a_four_byte_character_cut_at_a_chunk_end_go_whole_to_the_tail() {
    for (input, kept) in [("aé".as_bytes(), 1_usize), ("a😀".as_bytes(), 1)] {
        // One byte past the head is enough to cut each character's first byte.
        let head_cap = 2;
        let mut edges = OutputEdges::new(head_cap);
        let mut head = Vec::new();
        let chunk = Chunk {
            stream: Stream::Stdout,
            data: Bytes::copy_from_slice(&input[..head_cap]),
        };
        edges.feed(chunk, &mut |live| head.extend_from_slice(&live.data));

        assert_eq!(head.len(), kept, "{input:?}");
    }
}

#[test]
fn continuation_bytes_meeting_a_nearly_spent_head_are_cut_without_underflow() {
    // Three bytes of head left, then bytes that continue no character: the
    // old walk counted four continuations back from a cut at three.
    let mut edges = OutputEdges::new(EDGE_BYTES);
    let mut head = Vec::new();
    let filler = Chunk {
        stream: Stream::Stdout,
        data: Bytes::from(vec![b'a'; EDGE_BYTES - 3]),
    };
    edges.feed(filler, &mut |live| head.extend_from_slice(&live.data));
    let stray = Chunk {
        stream: Stream::Stdout,
        data: Bytes::from_static(b"\x80\x80\x80\x80"),
    };
    edges.feed(stray, &mut |live| head.extend_from_slice(&live.data));
    let mut tail = Vec::new();
    let omitted = edges.finish(&mut |kept| tail.extend_from_slice(&kept.data));

    assert_eq!(
        head.len(),
        EDGE_BYTES,
        "not text, so cut where the cap falls"
    );
    assert_eq!(tail, b"\x80", "nothing was dropped, so nothing is trimmed");
    assert_eq!(omitted, 0);
}

#[test]
fn every_short_head_against_stray_continuations_is_cut_in_range() {
    for left in 1..=4 {
        let input = [vec![b'a'; 8 - left], vec![0x80; 8]].concat();
        let (head, tail, omitted) = run(8, &input, 8 - left);

        assert_eq!(head.len(), 8, "head of {left}");
        assert_eq!(
            head.len() as u64 + tail.len() as u64 + omitted,
            input.len() as u64
        );
    }
}

#[test]
fn a_tail_opening_with_bytes_that_are_not_text_keeps_them() {
    // Undecodable bytes past one character's continuation are not a cut
    // character, so the tail keeps them.
    let input = [&b"abcd"[..], &[0xff; 5], b"z"].concat();
    let (head, tail, omitted) = run(4, &input, 4);

    assert_eq!(head, b"abcd");
    assert_eq!(tail, [&[0xff; 3][..], b"z"].concat());
    assert_eq!(omitted, 2, "only what the tail's cap dropped");
}

#[test]
fn a_cut_character_spread_over_short_tail_chunks_is_dropped_whole() {
    // The tail's cap drops the euro sign's first two bytes; its last byte is
    // a chunk of its own, ahead of the text that follows.
    let mut edges = OutputEdges::new(4);
    let mut head = Vec::new();
    let pieces: [&[u8]; 4] = [b"aaaa", b"\xe2\x82", b"\xac", b"zzz"];
    for piece in pieces {
        let chunk = Chunk {
            stream: Stream::Stdout,
            data: Bytes::copy_from_slice(piece),
        };
        edges.feed(chunk, &mut |live| head.extend_from_slice(&live.data));
    }
    let mut tail = Vec::new();
    let omitted = edges.finish(&mut |kept| tail.extend_from_slice(&kept.data));

    assert_eq!(head, b"aaaa");
    assert_eq!(tail, b"zzz", "the stray byte goes, and the tail is text");
    assert_eq!(omitted, 3);
}
