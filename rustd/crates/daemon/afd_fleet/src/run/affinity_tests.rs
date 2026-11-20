//! The assignment's token and its reading of a held thread, without a datastore.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the restriction set is for the daemon"
)]
use af_core::id::ENTROPY_LEN;

use super::*;

/// The assignment's instant, and a hold that is live at it.
const NOW: Timestamp = Timestamp::constant(1_767_225_600, 0);

/// A runner, distinct per `seed`.
fn runner(seed: u8) -> Uuid7 {
    Uuid7::encode(NOW, [seed; ENTROPY_LEN]).expect("a fixed instant encodes")
}

/// Who `winner` won a thread from, the thread last run by `holder`.
fn won(held: bool, holder: &Uuid7, winner: &Uuid7) -> Option<HeldAssignment> {
    held_assignment(held, Some(holder.as_str()), winner)
}

/// A live hold is the holder's to resume, and another runner's win over it
/// is counted as one.
#[test]
fn test_an_assignment_on_a_live_hold_counts_by_who_won() {
    let (holder, other) = (runner(1), runner(2));

    assert_eq!(won(true, &holder, &holder), Some(HeldAssignment::Holder));
    assert_eq!(
        won(true, &holder, &other),
        Some(HeldAssignment::OtherAfterLapse)
    );
}

/// A hold at or past its deadline binds nobody at the assignment's predicate,
/// so it is no hold here either: nobody resumes it and nothing counts.
#[test]
fn test_a_lapsed_or_absent_hold_is_no_hold() {
    let (holder, other) = (runner(1), runner(2));

    assert_eq!(won(false, &holder, &holder), None);
    assert_eq!(won(false, &holder, &other), None);
}

/// A fence only ever moves through the column's own type.
///
/// Cheap, and it is the property that makes the newtype worth its weight:
/// what goes into the row is what came out of the assignment.
#[test]
fn test_a_fence_round_trips_through_its_column_type() {
    let fence = Fence::from_i64(7);
    assert_eq!(fence.as_i64(), 7, "the token reaches the column unchanged");
}

/// Fences order, because that ordering IS the staleness test.
///
/// §3 rejects a report whose token is behind the thread's current one, so an
/// ordering that did not hold would be a stale writer admitted.
#[test]
fn test_a_later_fence_outranks_an_earlier_one() {
    assert!(
        Fence::from_i64(2) > Fence::from_i64(1),
        "a reassignment's token must outrank the holder it displaced"
    );
}

/// A release the datastore refused leaves the assignment to lapse, and its warning
/// carries the code as `error_code` and the failure's sentence as `reason`.
#[cfg(feature = "test-util")]
#[tokio::test]
async fn an_unreleased_assignment_reason_carries_no_code_prefix() {
    use crate::run::test_dead;

    let log = af_core::test_util::trace::Capture::install();
    test_dead::runs()
        .let_go(&test_dead::id(1), Fence::from_i64(3))
        .await;

    let line = log.only(EVENT_ASSIGNMENT_RELEASE_FAILED);
    let reason = line.field("reason").unwrap_or_default();
    assert!(
        line.field("error_code")
            .is_some_and(|code| !code.is_empty())
    );
    assert!(
        !reason.is_empty() && !reason.contains("[UZ-"),
        "the code is the line's `error_code`, never the reason's prefix: {reason}"
    );
}
