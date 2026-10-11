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

#[path = "keys_and_config/knobs.rs"]
mod knobs;
#[path = "keys_and_config/reconnect.rs"]
mod reconnect;

use std::time::Duration;

use afd_dragonfly::ready::{Partition, READY_INDEX_KEY, READY_PARTITIONS};
use afd_dragonfly::session::{SESSION_KEY_PREFIX, SESSION_TTL, session_key};
use afd_dragonfly::streams::{FLEET_CONSUMER_GROUP, fleet_stream_key};

/// The key every fleet's events live on, and the group they are read under.
///
/// The three names a Dragonfly key is built from, pinned as literals here
/// rather than read back from the constants they check, so the assertion
/// cannot drift with the thing it checks. They are worth asserting because a
/// producer and its consumers agree on these bytes or events vanish silently,
/// and a key renamed in a refactor strands every entry already written under
/// the old one.
#[test]
fn test_stream_key_and_group_are_pinned_literals() {
    assert_eq!(
        fleet_stream_key("fleet_0123"),
        "fleet:fleet_0123:events",
        "the producer and every consumer agree byte-for-byte or events vanish"
    );
    assert_eq!(
        FLEET_CONSUMER_GROUP, "fleet_lease",
        "every consumer reads under this group or a rename strands the backlog"
    );
    assert_eq!(
        READY_INDEX_KEY, "fleet:ready",
        "the readiness index moves and every entry already written is orphaned"
    );
    // pin test: literal is the contract
    assert_eq!(READY_PARTITIONS, 16, "a changed count re-homes every mark");
    assert_eq!(
        Partition::of("fleet_0123").key(),
        format!("fleet:ready:{{{}}}", Partition::of("fleet_0123").index()),
        "the partition key is the index key, a colon, and the number as a hash tag"
    );
}

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

/// Every reply shape a stream field can carry renders as readable text.
///
/// The producer writes bulk strings, so the other arms only run when something
/// upstream changed. Rendering one as an empty string would hand a caller a
/// field that looks absent rather than surprising.
#[test]
fn test_every_stream_field_reply_shape_renders() {
    let samples = afd_dragonfly::streams::rendered_field_samples();
    assert_eq!(samples.len(), 5, "a reply shape was added without a sample");

    for (label, rendered) in &samples {
        assert!(!rendered.is_empty(), "{label} rendered as nothing");
    }
    let by_label = |wanted: &str| {
        samples
            .iter()
            .find(|(label, _)| *label == wanted)
            .map(|(_, rendered)| rendered.as_str())
            .expect("sample present")
    };
    assert_eq!(by_label("bulk string"), "ready");
    assert_eq!(by_label("simple string"), "OK");
    assert_eq!(by_label("integer"), "42");
    assert_eq!(
        by_label("anything else"),
        "nil",
        "an unrecognised value keeps its shape visible through Debug"
    );
    assert_eq!(
        by_label("invalid utf-8"),
        "binary-data([255, 254])",
        "a field this daemon did not write shows its bytes rather than a \
         sentence with replacement characters punched through it"
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
