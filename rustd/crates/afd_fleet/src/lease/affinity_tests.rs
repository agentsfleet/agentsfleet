//! The claim's token and its reading of a held slot, without a datastore.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the restriction set is for the daemon"
)]
use afd_core::id::ENTROPY_LEN;

use super::*;

/// The claim's instant, and a hold that is live at it.
const NOW: i64 = 1_767_225_600_000;
const LIVE: Option<i64> = Some(NOW + 1);

/// A runner, distinct per `seed`.
fn runner(seed: u8) -> Uuid7 {
    Uuid7::encode(UnixMillis::from_millis(NOW), [seed; ENTROPY_LEN])
        .expect("a fixed instant encodes")
}

/// Who `claimer` won a slot from, the slot last run by `holder`.
fn won(held_until: Option<i64>, holder: &Uuid7, claimer: &Uuid7) -> Option<HeldClaim> {
    held_claim(
        held_until,
        Some(holder.as_str()),
        claimer,
        UnixMillis::from_millis(NOW),
    )
}

/// A live hold is the holder's to resume, and another runner's win over it
/// is counted as one.
#[test]
fn test_a_claim_on_a_live_hold_counts_by_who_won() {
    let (holder, other) = (runner(1), runner(2));

    assert_eq!(won(LIVE, &holder, &holder), Some(HeldClaim::Holder));
    assert_eq!(won(LIVE, &holder, &other), Some(HeldClaim::OtherAfterLapse));
}

/// A hold at or past its deadline binds nobody at the claim's predicate,
/// so it is no hold here either: nobody resumes it and nothing counts.
#[test]
fn test_a_lapsed_or_absent_hold_is_no_hold() {
    let (holder, other) = (runner(1), runner(2));

    for lapsed in [Some(NOW), Some(NOW - 1), None] {
        assert_eq!(won(lapsed, &holder, &holder), None, "{lapsed:?}");
        assert_eq!(won(lapsed, &holder, &other), None, "{lapsed:?}");
    }
}

/// A fence only ever moves through the column's own type.
///
/// Cheap, and it is the property that makes the newtype worth its weight:
/// what goes into the row is what came out of the claim.
#[test]
fn test_a_fence_round_trips_through_its_column_type() {
    let fence = Fence::from_i64(7);
    assert_eq!(fence.as_i64(), 7, "the token reaches the column unchanged");
}

/// Fences order, because that ordering IS the staleness test.
///
/// §3 rejects a report whose token is behind the slot's current one, so an
/// ordering that did not hold would be a stale writer admitted.
#[test]
fn test_a_later_fence_outranks_an_earlier_one() {
    assert!(
        Fence::from_i64(2) > Fence::from_i64(1),
        "a reclaim's token must outrank the holder it displaced"
    );
}
