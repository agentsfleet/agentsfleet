//! When a runner counts as gone, and when it has simply not arrived.
//!
//! The staleness predicate is the whole of Dimension 6.3 and it needs no
//! datastore: what it reads is a column and a clock, both of which are
//! parameters.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_core::timing::RUNNER_OFFLINE_AFTER_MS;
use afd_wire::admin::AdminState;

use super::Due;
use crate::sql;

/// The instant every case below is judged at.
const NOW_MS: i64 = 1_760_000_000_000;

/// A due row for a runner last seen at `last_seen_at`.
fn seen_at(last_seen_at: i64) -> Due {
    Due {
        id: Uuid7::parse("0195b4ba-8d3a-7f13-8abc-2b3e1e0c1a01").expect("a canonical v7 id"),
        last_seen_at,
        admin_state: Some(AdminState::Active),
    }
}

#[test]
fn a_runner_that_has_never_connected_is_not_stale() {
    // Dimension 6.3. `now - 0` is decades, so arithmetic alone would report a
    // runner enrolled a second ago as long dead — offline before it ever had a
    // chance to beat. A runner that has never connected is not one that
    // stopped; it is one that has not started, and that is a different word to
    // an operator and a different row on a dashboard.
    let fresh = seen_at(sql::LAST_SEEN_NEVER);
    assert!(!fresh.is_stale(UnixMillis::from_millis(NOW_MS)));
}

#[test]
fn a_runner_is_stale_only_once_it_is_past_the_threshold() {
    let now = UnixMillis::from_millis(NOW_MS);
    // Exactly at the threshold is not past it: the comparison is `>`, so a
    // runner whose beat is due this instant gets the instant.
    assert!(!seen_at(NOW_MS - RUNNER_OFFLINE_AFTER_MS).is_stale(now));
    assert!(seen_at(NOW_MS - RUNNER_OFFLINE_AFTER_MS - 1).is_stale(now));
    // And one that beat a moment ago is plainly alive.
    assert!(!seen_at(NOW_MS - 1).is_stale(now));
}

#[test]
fn a_clock_that_went_backwards_does_not_make_a_runner_stale() {
    // A last-seen stamp in the FUTURE is what an NTP correction or a clock skew
    // between two daemon instances produces. Saturating arithmetic reads it as
    // zero elapsed rather than as an enormous positive — the safe direction,
    // because the alternative reports a live runner offline and releases the
    // slots it is actively using.
    let now = UnixMillis::from_millis(NOW_MS);
    assert!(!seen_at(NOW_MS + RUNNER_OFFLINE_AFTER_MS).is_stale(now));
}
