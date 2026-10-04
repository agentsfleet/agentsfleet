#![expect(
    clippy::panic,
    reason = "test module: one test fails for its own reason to prove the frame guard stands down"
)]

use std::time::Instant;

use afd_wire::activity::{ActivityFrame, StreamTextKind};

use super::Live;
use crate::fixture::{Frames, GITHUB_TOKEN, scrub};

/// Each chunk's text and sequence, in order.
fn chunks(frames: &[ActivityFrame<'_>]) -> Vec<(String, u64)> {
    frames
        .iter()
        .filter_map(|frame| match frame {
            ActivityFrame::FleetResponseChunk(chunk) => {
                Some((chunk.text.to_string(), chunk.stream_seq))
            }
            _other => None,
        })
        .collect()
}

#[test]
fn should_hold_a_secret_split_across_chunks_and_drop_its_head_at_the_end() {
    let scrub = scrub();
    let frames = Frames::default();
    let sink = frames.sink();
    let mut live = Live::new(&sink, &scrub, Instant::now());
    let (head, tail) = GITHUB_TOKEN.split_at(4);

    live.text(StreamTextKind::Answer, &format!("a {head}"));
    live.text(StreamTextKind::Answer, &format!("{tail} b {head}"));
    live.end_pass();
    live.text(StreamTextKind::Answer, "c");

    assert_eq!(
        chunks(&frames.taken()),
        [
            ("a ".to_owned(), 0),
            ("«secret:github.token» b ".to_owned(), 1),
            ("c".to_owned(), 2)
        ]
    );
}

#[test]
fn should_send_nothing_for_a_chunk_held_whole() {
    let scrub = scrub();
    let frames = Frames::default();
    let sink = frames.sink();
    let mut live = Live::new(&sink, &scrub, Instant::now());

    live.text(StreamTextKind::Reasoning, &GITHUB_TOKEN[..3]);

    assert!(frames.taken().is_empty(), "no frame and no sequence spent");
}

#[test]
#[should_panic(expected = "frames emitted and never read")]
fn the_frame_guard_fails_a_test_that_leaves_a_frame_unread() {
    let scrub = scrub();
    let frames = Frames::default();
    let sink = frames.sink();
    let mut live = Live::new(&sink, &scrub, Instant::now());

    live.text(StreamTextKind::Answer, "said and never asserted");
}

/// What a test failing for its own reason panics with.
const OWN_FAILURE: &str = "the test's own assertion failed";

/// The guard stands down while a test is already failing: a second panic
/// during the unwind would abort the whole suite and hide the first.
#[test]
#[should_panic(expected = "the test's own assertion failed")]
fn the_frame_guard_stands_down_while_a_test_is_already_failing() {
    let scrub = scrub();
    let frames = Frames::default();
    let sink = frames.sink();
    let mut live = Live::new(&sink, &scrub, Instant::now());

    live.text(StreamTextKind::Answer, "said and never asserted");
    std::panic::panic_any(OWN_FAILURE);
}
