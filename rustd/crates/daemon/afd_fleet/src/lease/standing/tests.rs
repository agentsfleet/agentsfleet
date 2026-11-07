//! The fence rule every lease-addressed verb applies.

use super::LiveLease;

/// The fleet's live sequence a fixture lease runs under.
const LIVE: i64 = 4;

/// A lease whose own token is `fence`, under a fleet at `live_seq`.
fn lease(fence: i64, live_seq: i64) -> LiveLease {
    LiveLease {
        fleet_id: "01924f4e-0000-7000-8000-00000000fee7".to_owned(),
        workspace_id: "01924f4e-0000-7000-8000-000000000001".to_owned(),
        event_id: "1700000000000-0".to_owned(),
        actor: "steer:api".to_owned(),
        fence,
        live_seq,
    }
}

#[test]
fn the_current_holder_presenting_its_own_token_holds() {
    assert!(lease(LIVE, LIVE).holds(4));
}

/// A reclaim bumped the fleet's sequence past this lease's token.
#[test]
fn a_superseded_holder_does_not() {
    assert!(!lease(LIVE - 1, LIVE).holds(3));
}

/// The lease is current, but the token presented is not its own.
#[test]
fn a_token_that_is_not_the_lease_s_own_does_not() {
    assert!(!lease(LIVE, LIVE).holds(5));
    assert!(!lease(LIVE, LIVE).holds(3));
}

/// A stored token no sequence can be: never a holder.
#[test]
fn a_negative_stored_token_never_holds() {
    assert!(!lease(-1, -2).holds(0));
}
