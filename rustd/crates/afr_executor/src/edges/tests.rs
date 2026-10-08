#![expect(
    clippy::indexing_slicing,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use bytes::Bytes;

use super::{Chunk, EDGE_BYTES, Next, Unread};
use crate::api::Stream;

/// One kibibyte.
const KIB: usize = 1024;
/// One mebibyte.
const MIB: usize = KIB * KIB;

/// A stdout chunk of `data`.
pub(super) fn stdout(data: &[u8]) -> Chunk {
    Chunk {
        stream: Stream::Stdout,
        data: Bytes::copy_from_slice(data),
    }
}

/// Everything a reader takes from `unread`, in order.
pub(super) fn drain(unread: &mut Unread) -> Vec<Next> {
    std::iter::from_fn(|| unread.pop()).collect()
}

/// Feeds `input` in `piece`-sized chunks with no read between, then reads it
/// all: the bytes before the gap, the bytes after it, and the gap.
fn run(edge: usize, input: &[u8], piece: usize) -> (Vec<u8>, Vec<u8>, u64) {
    let mut unread = Unread::new(edge);
    for slice in input.chunks(piece) {
        unread.push(stdout(slice));
    }
    let (mut before, mut after, mut omitted) = (Vec::new(), Vec::new(), 0);
    for next in drain(&mut unread) {
        match next {
            Next::Output(chunk) if omitted == 0 => before.extend_from_slice(&chunk.data),
            Next::Output(chunk) => after.extend_from_slice(&chunk.data),
            Next::Omitted(bytes) => omitted = bytes,
        }
    }
    (before, after, omitted)
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
fn output_under_the_head_is_read_whole_and_nothing_is_omitted() {
    let (head, tail, omitted) = run(16, b"short", 2);

    assert_eq!(head, b"short");
    assert_eq!(tail, [] as [u8; 0]);
    assert_eq!(omitted, 0);
}

#[test]
fn output_between_the_edges_keeps_every_byte() {
    let input: Vec<u8> = (0..24).collect();
    let (read, after, omitted) = run(16, &input, 5);

    assert_eq!(read, input);
    assert_eq!(after, [] as [u8; 0]);
    assert_eq!(omitted, 0);
}

#[test]
fn a_chunk_that_starts_inside_a_character_never_queues_an_empty_head() {
    // The head has one byte left and the chunk opens with a two-byte
    // character, so the cut moves back to zero and the head stays empty.
    let mut unread = Unread::new(1);
    unread.push(Chunk {
        stream: Stream::Terminal,
        data: Bytes::from_static("é!".as_bytes()),
    });

    assert_eq!(
        drain(&mut unread),
        [
            Next::Omitted(2),
            Next::Output(Chunk {
                stream: Stream::Terminal,
                data: Bytes::from_static(b"!"),
            }),
        ],
        "the character that no longer fits either edge is counted"
    );
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
    let mut unread = Unread::new(4);
    for slice in "aa€b".as_bytes().chunks(4) {
        unread.push(stdout(slice));
    }

    let read = drain(&mut unread);

    assert_eq!(read.first(), Some(&Next::Output(stdout(b"aa"))));
    let rest: Vec<u8> = read
        .iter()
        .skip(1)
        .filter_map(|next| match next {
            Next::Output(chunk) => Some(chunk.data.to_vec()),
            Next::Omitted(_) => None,
        })
        .flatten()
        .collect();
    assert_eq!(rest, "€b".as_bytes());
    assert_eq!(
        read.len(),
        3,
        "the head, then the tail, and nothing fell between"
    );
}

#[test]
fn a_chunk_ending_on_a_complete_character_is_kept_whole_in_the_head() {
    let input = "a€".as_bytes();
    let (head, tail, omitted) = run(4, input, 4);

    assert_eq!(head, input);
    assert_eq!(tail, [] as [u8; 0]);
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
    for input in ["aé".as_bytes(), "a😀".as_bytes()] {
        // One byte past the head is enough to cut each character's first byte.
        let head_cap = 2;
        let mut unread = Unread::new(head_cap);
        unread.push(stdout(&input[..head_cap]));

        assert_eq!(unread.pop(), Some(Next::Output(stdout(b"a"))), "{input:?}");
    }
}

#[test]
fn continuation_bytes_meeting_a_nearly_spent_head_are_cut_without_underflow() {
    // Three bytes of head left, then bytes that continue no character: the
    // old walk counted four continuations back from a cut at three.
    let mut unread = Unread::new(EDGE_BYTES);
    unread.push(stdout(&vec![b'a'; EDGE_BYTES - 3]));
    unread.push(stdout(b"\x80\x80\x80\x80"));

    let read = drain(&mut unread);

    assert_eq!(
        read.get(1),
        Some(&Next::Output(stdout(b"\x80\x80\x80"))),
        "not text, so cut where the cap falls"
    );
    assert_eq!(
        read.last(),
        Some(&Next::Output(stdout(b"\x80"))),
        "nothing was dropped, so nothing is trimmed"
    );
    assert_eq!(read.len(), 3);
}

#[test]
fn every_short_head_against_stray_continuations_is_cut_in_range() {
    for left in 1..=4 {
        let input = [vec![b'a'; 8 - left], vec![0x80; 8]].concat();
        let (head, tail, omitted) = run(8, &input, 8 - left);

        assert_eq!(
            head.len() as u64 + tail.len() as u64 + omitted,
            input.len() as u64,
            "head of {left}"
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
    let mut unread = Unread::new(4);
    let pieces: [&[u8]; 4] = [b"aaaa", b"\xe2\x82", b"\xac", b"zzz"];
    for piece in pieces {
        unread.push(stdout(piece));
    }

    assert_eq!(
        drain(&mut unread),
        [
            Next::Output(stdout(b"aaaa")),
            Next::Omitted(3),
            Next::Output(stdout(b"zzz")),
        ],
        "the stray byte goes, and the tail is text"
    );
}
