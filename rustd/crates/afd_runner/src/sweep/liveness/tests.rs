//! When a runner counts as gone, and when it has simply not arrived.
//!
//! The staleness predicate is the whole of Dimension 6.3 and it needs no
//! datastore: what it reads is a column and the clock the fetch answered with,
//! both of which are fields.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use std::time::Duration;

use afd_api_wire::admin::AdminState;
use afd_core::clock;
use afd_core::id::Uuid7;
use afd_core::timing::RUNNER_OFFLINE_AFTER;
use jiff::Timestamp;

use super::Due;
use crate::sql;

/// The instant every case below is judged at.
const NOW: Timestamp = Timestamp::constant(1_760_000_000, 0);

/// The finest step an instant takes on the wire and in the column.
const ONE_MILLISECOND: Duration = Duration::from_millis(1);

/// A due row for a runner last seen at `last_seen_at`, fetched at [`NOW`].
fn seen_at(last_seen_at: Timestamp) -> Due {
    Due {
        id: Uuid7::parse("0195b4ba-8d3a-7f13-8abc-2b3e1e0c1a01").expect("a canonical v7 id"),
        last_seen_at,
        checked_at: NOW,
        status: Some(AdminState::Active),
    }
}

#[test]
fn a_runner_that_has_never_connected_is_not_stale() {
    // Dimension 6.3. The sentinel is the epoch, decades before any fetch, so
    // arithmetic alone would report a runner enrolled a second ago as long
    // dead — offline before it ever had a chance to beat. A runner that has
    // never connected is not one that stopped; it is one that has not started,
    // and that is a different word to an operator and a different row on a
    // dashboard.
    assert!(!seen_at(sql::NEVER_SEEN).is_stale());
}

#[test]
fn a_runner_is_stale_only_once_it_is_past_the_threshold() {
    let threshold = clock::before(NOW, RUNNER_OFFLINE_AFTER);
    // Exactly at the threshold is not past it: the comparison is strict, so a
    // runner whose beat is due this instant gets the instant.
    assert!(!seen_at(threshold).is_stale());
    assert!(seen_at(clock::before(threshold, ONE_MILLISECOND)).is_stale());
    // And one that beat a moment ago is plainly alive.
    assert!(!seen_at(clock::before(NOW, ONE_MILLISECOND)).is_stale());
}

#[test]
fn a_clock_that_went_backwards_does_not_make_a_runner_stale() {
    // A last-seen stamp in the FUTURE is what an NTP correction or a clock skew
    // between two database replicas produces. It reads as no time elapsed —
    // the safe direction, because the alternative reports a live runner
    // offline and releases the slots it is actively using.
    assert!(!seen_at(clock::after(NOW, RUNNER_OFFLINE_AFTER)).is_stale());
}
