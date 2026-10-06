//! Reads between writes: a reader that keeps up, one that caught up, and one
//! that reads now and then.

use super::tests::{drain, stdout};
use super::{Next, Unread};

/// The bytes a reader took, and the bytes it was told fell between.
fn tally(read: &[Next]) -> (Vec<u8>, u64) {
    read.iter()
        .fold((Vec::new(), 0), |(mut bytes, omitted), next| match next {
            Next::Output(chunk) => {
                bytes.extend_from_slice(&chunk.data);
                (bytes, omitted)
            }
            Next::Omitted(gap) => (bytes, omitted + gap),
        })
}

#[test]
fn a_reader_that_keeps_up_loses_nothing() {
    let input: Vec<u8> = (0..=250_u8).cycle().take(7000).collect();
    let mut unread = Unread::new(16);
    let mut read = Vec::new();

    for slice in input.chunks(7) {
        unread.push(stdout(slice));
        read.extend(drain(&mut unread));
    }

    assert_eq!(tally(&read), (input, 0));
}

#[test]
fn a_reader_that_caught_up_gets_a_whole_head_again() {
    let mut unread = Unread::new(4);
    unread.push(stdout(b"aaaabbbbcccc"));
    let first = drain(&mut unread);
    unread.push(stdout(b"ddddeeee"));

    assert_eq!(
        first,
        [
            Next::Output(stdout(b"aaaa")),
            Next::Omitted(4),
            Next::Output(stdout(b"cccc")),
        ]
    );
    assert_eq!(
        drain(&mut unread),
        [Next::Output(stdout(b"dddd")), Next::Output(stdout(b"eeee"))],
        "eight bytes fit a fresh head and tail, so none is dropped"
    );
}

#[test]
fn a_tail_read_into_the_head_counts_against_it() {
    let mut unread = Unread::new(4);
    unread.push(stdout(b"aaaabbbbcccc"));
    let crossed = [unread.pop(), unread.pop()];
    unread.push(stdout(b"dddd"));
    unread.push(stdout(b"eeee"));

    assert_eq!(
        crossed,
        [Some(Next::Output(stdout(b"aaaa"))), Some(Next::Omitted(4))]
    );
    assert_eq!(
        drain(&mut unread),
        [
            Next::Output(stdout(b"cccc")),
            Next::Omitted(4),
            Next::Output(stdout(b"eeee")),
        ],
        "the old tail fills the head, so new output lands in the tail"
    );
}

#[test]
fn unread_output_never_exceeds_two_edges_and_every_byte_is_read_or_counted() {
    let edge = 64;
    let mut unread = Unread::new(edge);
    let mut pushed = 0_u64;
    let mut read = Vec::new();

    for step in 0..2000_usize {
        let size = (step * 37) % 101 + 1;
        unread.push(stdout(&vec![b'x'; size]));
        pushed += size as u64;
        if step % 7 == 0 {
            read.extend((0..step % 5).map_while(|_| unread.pop()));
        }

        let head: usize = unread.head.iter().map(|chunk| chunk.data.len()).sum();
        assert!(head + unread.tail_len <= 2 * edge, "step {step}");
    }
    read.extend(drain(&mut unread));

    let (bytes, omitted) = tally(&read);
    assert_eq!(bytes.len() as u64 + omitted, pushed);
    assert!(
        omitted > 0,
        "the reader fell behind, so something was dropped"
    );
}

#[test]
fn nothing_unread_reads_as_none() {
    let mut unread = Unread::new(4);

    assert_eq!(unread.pop(), None);
    unread.push(stdout(b"a"));
    assert_eq!(unread.pop(), Some(Next::Output(stdout(b"a"))));
    assert_eq!(unread.pop(), None);
}

/// A character the head starts and a gap cuts off joins the gap, so the
/// reader never gets a lead byte with nothing after it.
#[test]
fn a_character_the_head_starts_before_a_gap_joins_the_gap() {
    let mut unread = Unread::new(4);
    unread.push(stdout(b"aaaa"));
    unread.push(stdout(b"cc"));
    unread.push(stdout(b"b\xe2\x82"));
    assert_eq!(unread.pop(), Some(Next::Output(stdout(b"aaaa"))));
    assert_eq!(
        unread.pop(),
        Some(Next::Omitted(1)),
        "the tail made room once"
    );
    // The old tail is the head now, full, and ends two bytes into a euro
    // sign; the sign's last byte lands in the tail and is dropped from it.
    unread.push(stdout(b"\xac"));
    unread.push(stdout(b"dddd"));

    assert_eq!(
        drain(&mut unread),
        [
            Next::Output(stdout(b"c")),
            Next::Output(stdout(b"b")),
            Next::Omitted(3),
            Next::Output(stdout(b"dddd")),
        ],
        "the two bytes the sign started with join the gap"
    );
}
