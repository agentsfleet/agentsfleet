//! The shapes every replica has to agree on, checked without a server.
//!
//! Every key format, knob name and constant here is read or written by every
//! replica against one Dragonfly, and by the entries already in it. A drift in
//! any of them is not a failed test in production — it is two processes
//! quietly using different keys against the same Dragonfly, which looks like
//! lost events rather than like a bug.
#![cfg(feature = "test-util")]
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

mod knobs;
mod reconnect;

use std::time::Duration;

use afd_dragonfly::session::{SESSION_KEY_PREFIX, SESSION_TTL, session_key};

/// The session key and time-to-live, pinned as literals.
///
/// A prefix or a lifetime that moves without a migration signs every live
/// session out at once.
#[test]
fn test_session_key_and_ttl_are_pinned_literals() {
    assert_eq!(session_key("abc"), "auth:session:abc");
    assert_eq!(SESSION_TTL, Duration::from_secs(300));

    assert_eq!(
        SESSION_KEY_PREFIX, "auth:session:",
        "a moved prefix signs every live session out at once"
    );
}

/// Every abort reason spells itself the way the stored value reads.
///
/// The reason rides `session:*` in Dragonfly and BOTH binaries read it back. A
/// respelling here is not a failed test in production — it is one process
/// writing a reason the other cannot classify, on a record that exists to say
/// why somebody's run was stopped.
#[test]
fn every_abort_reason_carries_its_stored_spelling() {
    use afd_dragonfly::session::AbortReason;

    assert_eq!(AbortReason::ExplicitCancel.as_str(), "explicit_cancel");
    assert_eq!(
        AbortReason::RateLimitExceeded.as_str(),
        "rate_limit_exceeded"
    );
    assert_ne!(
        AbortReason::ExplicitCancel.as_str(),
        AbortReason::RateLimitExceeded.as_str(),
        "a person cancelling and a ceiling stopping them are different facts"
    );
}
