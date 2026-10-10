//! The fence a memory verb decides through: both columns, one rule.
#![expect(
    clippy::expect_used,
    reason = "a test fails loudly on a fixture it cannot build"
)]

use afd_core::error_code;

use super::Fence;

/// The fleet's live sequence a fixture lease runs under.
const LIVE: i64 = 6;

/// A fence that is known to be well formed.
fn fence(own: i64, live_seq: i64) -> Fence {
    Fence::new(own, live_seq).expect("both columns are sequences")
}

#[test]
fn a_lease_the_fleet_has_not_moved_past_is_current() {
    assert!(fence(LIVE, LIVE).current());
}

/// A won claim bumped the sequence before it expired this lease's row.
#[test]
fn a_lease_the_fleet_has_moved_past_is_not_current() {
    assert!(!fence(LIVE - 1, LIVE).current());
}

#[test]
fn only_the_leases_own_token_holds() {
    let current = fence(LIVE, LIVE);
    assert!(current.holds(6));
    assert!(!current.holds(5), "a lower token is a superseded holder's");
    assert!(!current.holds(7), "a higher token is no holder's");
    assert!(!current.holds(u64::MAX), "nor is the highest");
}

#[test]
fn a_superseded_lease_holds_with_no_token() {
    let superseded = fence(LIVE - 1, LIVE);
    for presented in [5, 6, u64::MAX] {
        assert!(!superseded.holds(presented), "{presented}");
    }
}

/// Reported as the datastore fault it is, never read as a fence.
#[test]
fn a_negative_column_is_a_corrupt_sequence() {
    for (own, live_seq) in [(-1, LIVE), (LIVE, -1)] {
        let refused = Fence::new(own, live_seq).expect_err("no sequence is negative");
        assert_eq!(
            refused.code(),
            error_code::INTERNAL_DB_QUERY,
            "{own}, {live_seq}"
        );
    }
}
