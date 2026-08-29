//! The operator-facing names a waiting verdict carries.
//!
//! A unit file rather than a case inside the lifecycle suite: these are values
//! with no datastore anywhere in reach, and the filename is what says so —
//! `integration_gate_lifecycle.rs` needs Postgres and Redis, this needs a
//! compiler. Splitting them is what lets the unit lane run this one on a
//! machine with Docker closed.
//!
//! The names are wire-visible: an operator reads them in a queue listing, so a
//! rename here is a rename in somebody's dashboard.
#![cfg(feature = "test-util")]

use afd_gate::gate::Waiting;

#[test]
fn waiting_states_have_stable_operator_names() {
    assert_eq!(Waiting::Parked.as_str(), "parked");
    assert_eq!(Waiting::Pending.as_str(), "pending");
    assert_eq!(Waiting::Unreadable.as_str(), "unreadable");
}
