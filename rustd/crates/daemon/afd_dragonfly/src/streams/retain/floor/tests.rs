//! The floor's decisions that need no datastore: where a window cuts, when a
//! stream is past its slack, and how much one trim may read.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the restriction set is for the daemon"
)]

use super::{TRIM_READ_MAX, excess_of, floor_of, window_size};
use crate::streams::retain::{ACKNOWLEDGED_HISTORY, Position, TRIM_SLACK};

/// A position from its millisecond, sequence zero.
fn at(millis: u64) -> Position {
    Position {
        millis,
        sequence: 0,
    }
}

/// The owed position every window below is read up to.
const OWED: u64 = 100;

/// How many entries the windows below ask for.
const WANTED: usize = 3;

/// A full window ended at the oldest entry the history keeps, which is the
/// floor: everything before it goes, and it stays.
#[test]
fn a_full_window_cuts_at_its_last_entry() {
    let window = [at(1), at(2), at(3)];
    assert_eq!(floor_of(at(OWED), &window, WANTED), Some(at(3)));
}

/// A short window read everything up to the owed position, so the owed
/// position is the floor and the history window above it is untouched.
#[test]
fn a_short_window_cuts_at_the_owed_position() {
    let window = [at(1), at(2)];
    assert_eq!(floor_of(at(OWED), &window, WANTED), Some(at(OWED)));
}

/// Nothing below the floor means no `XTRIM` at all: an empty window, and a
/// window whose only entry IS the owed one, which the floor keeps.
#[test]
fn a_window_with_nothing_below_its_floor_cuts_nothing() {
    assert_eq!(floor_of(at(OWED), &[], WANTED), None);
    assert_eq!(floor_of(at(OWED), &[at(OWED)], WANTED), None);
}

/// A stream inside its bound plus the slack is not trimmed; one entry more is,
/// by exactly how far it is past the bound.
#[test]
fn a_stream_inside_its_slack_is_not_trimmed() {
    let bound = u64::try_from(ACKNOWLEDGED_HISTORY + TRIM_SLACK).expect("fits");
    assert_eq!(excess_of(0, ACKNOWLEDGED_HISTORY), None);
    assert_eq!(excess_of(bound, ACKNOWLEDGED_HISTORY), None);
    assert_eq!(
        excess_of(bound + 1, ACKNOWLEDGED_HISTORY),
        Some(TRIM_SLACK + 1)
    );
}

/// One trim reads the excess plus the entry it stops at, and never more than
/// the cap plus that one, however far a stream fell behind.
#[test]
fn one_trim_reads_at_most_the_cap() {
    assert_eq!(window_size(TRIM_SLACK + 1), TRIM_SLACK + 2);
    assert_eq!(window_size(TRIM_READ_MAX * 5), TRIM_READ_MAX + 1);
}
