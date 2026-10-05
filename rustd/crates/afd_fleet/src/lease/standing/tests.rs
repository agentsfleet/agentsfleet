//! The fence rule every lease-addressed verb applies.

use super::holds;

/// The fleet's live sequence a fixture lease runs under.
const LIVE: i64 = 4;

#[test]
fn the_current_holder_presenting_its_own_token_holds() {
    assert!(holds(LIVE, LIVE, 4));
}

/// A reclaim bumped the fleet's sequence past this lease's token.
#[test]
fn a_superseded_holder_does_not() {
    assert!(!holds(LIVE - 1, LIVE, 3));
}

/// The lease is current, but the token presented is not its own.
#[test]
fn a_token_that_is_not_the_lease_s_own_does_not() {
    assert!(!holds(LIVE, LIVE, 5));
    assert!(!holds(LIVE, LIVE, 3));
}

/// A stored token no sequence can be: never a holder.
#[test]
fn a_negative_stored_token_never_holds() {
    assert!(!holds(-1, -2, 0));
}
