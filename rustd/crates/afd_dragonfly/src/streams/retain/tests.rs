//! The parts of retention that decide something without a datastore: the
//! ordering a floor is computed under, and what a backlog answers.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the restriction set is for the daemon"
)]

use super::{Backlog, Position};

/// Two ids that text orders one way and integers the other: the shorter
/// millisecond sorts AFTER the longer one lexically.
const EARLY_ID: &str = "999-0";
const LATE_ID: &str = "1000-0";
/// The next sequence inside [`LATE_ID`]'s millisecond.
const LATE_NEXT_ID: &str = "1000-1";

/// Integer ordering, never lexical: the failure this pins is a floor
/// computed from text, which would trim below [`LATE_ID`] when asked to keep
/// [`EARLY_ID`].
#[test]
fn positions_order_by_their_integers_not_their_text() {
    let early = Position::parse(EARLY_ID).expect("a well-formed id parses");
    let late = Position::parse(LATE_ID).expect("a well-formed id parses");
    assert!(
        early < late,
        "the earlier millisecond precedes the later whatever the text says"
    );
    assert!(
        Position::parse(LATE_NEXT_ID).expect("parses") > late,
        "the sequence breaks ties inside one millisecond"
    );
    assert_eq!(late.render(), LATE_ID, "the id renders as Redis spelled it");
}

/// A reply that is not an id is refused, not ordered somewhere.
#[test]
fn a_malformed_id_is_refused() {
    // pin test: literal is the contract
    for malformed in ["", "1000", "a-b", "1000-", "-0"] {
        assert!(
            Position::parse(malformed).is_err(),
            "{malformed:?} is not a stream id"
        );
    }
}

/// The outstanding count is answerable only when the server vouched for
/// the undelivered half; deliverability reads the unknown as "maybe".
#[test]
fn an_unknown_lag_is_deliverable_and_not_countable() {
    let unknown = Backlog {
        pending: 0,
        undelivered: None,
    };
    assert_eq!(unknown.outstanding(), None);
    assert!(unknown.is_deliverable(), "an unknown lag may hide work");

    let drained = Backlog {
        pending: 0,
        undelivered: Some(0),
    };
    assert_eq!(drained.outstanding(), Some(0));
    assert!(!drained.is_deliverable());

    let owed = Backlog {
        pending: 2,
        undelivered: Some(3),
    };
    assert_eq!(owed.outstanding(), Some(5));
    assert!(owed.is_deliverable());
}
