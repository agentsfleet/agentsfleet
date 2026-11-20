//! The fence rule every run-addressed verb applies.

use af_core::id::Uuid7;

use super::LiveRun;

/// A test's outcome: a fixture identifier that does not parse fails it.
type Checked<T = ()> = af_core::error::Result<T>;

/// The fleet's live sequence a fixture run runs under.
const LIVE: i64 = 4;

/// A run whose own token is `fence`, under a fleet at `live_seq`.
fn payload(fence: i64, live_seq: i64) -> Checked<LiveRun> {
    Ok(LiveRun {
        fleet_id: Uuid7::parse("01924f4e-0000-7000-8000-00000000fee7")?,
        workspace_id: Uuid7::parse("01924f4e-0000-7000-8000-000000000001")?,
        turn_id: Uuid7::parse("01924f4e-0000-7000-8000-0000000000e9")?,
        actor: "steer:api".to_owned(),
        fence,
        live_seq,
    })
}

#[test]
fn the_current_holder_presenting_its_own_token_holds() -> Checked {
    assert!(payload(LIVE, LIVE)?.holds(4));
    Ok(())
}

/// A reassignment bumped the fleet's sequence past this run's token.
#[test]
fn a_superseded_holder_does_not() -> Checked {
    assert!(!payload(LIVE - 1, LIVE)?.holds(3));
    Ok(())
}

/// The run is current, but the token presented is not its own.
#[test]
fn a_token_that_is_not_the_run_s_own_does_not() -> Checked {
    assert!(!payload(LIVE, LIVE)?.holds(5));
    assert!(!payload(LIVE, LIVE)?.holds(3));
    Ok(())
}

/// A stored token no sequence can be: never a holder.
#[test]
fn a_negative_stored_token_never_holds() -> Checked {
    assert!(!payload(-1, -2)?.holds(0));
    Ok(())
}
