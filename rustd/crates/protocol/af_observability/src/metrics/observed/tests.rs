//! Dimension 3.5 — an invalid snapshot emits no data point, never a zero.
//!
//! The assertions are about the DIFFERENCE between absent and zero, because
//! that is the whole reason the type exists. A cell that reported `0` when
//! nothing had been published would be indistinguishable, on a dashboard, from
//! a queue that is genuinely empty — and an operator would read a failed
//! collection as a healthy system.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use std::sync::Arc;
use std::thread;

use super::Observed;

/// A cell nobody has published into observes nothing. Not zero — nothing.
#[test]
fn test_observed_absent_never_zero() {
    let cell = Observed::new();
    assert_eq!(
        cell.load(),
        None,
        "an unpublished cell must decline to observe, not report a reading \
         nobody took"
    );
}

/// A published reading is what the callback loads.
#[test]
fn test_a_published_reading_is_observed() {
    let cell = Observed::new();
    cell.publish(42);
    assert_eq!(cell.load(), Some(42));
}

/// Zero is a legitimate reading and survives as one. This is the other side of
/// the rule: absent must not become zero, and a real zero must not become
/// absent — an empty queue is a fact worth publishing.
#[test]
fn test_a_published_zero_is_a_reading_not_an_absence() {
    let cell = Observed::new();
    cell.publish(0);
    assert_eq!(
        cell.load(),
        Some(0),
        "a measured zero is a reading; only an unpublished cell is absent"
    );
}

/// A publisher whose own read failed withdraws, and the callback goes back to
/// observing nothing rather than repeating the last good value forever.
#[test]
fn test_a_withdrawn_cell_stops_observing() {
    let cell = Observed::new();
    cell.publish(7);
    assert_eq!(cell.load(), Some(7));

    cell.withdraw();
    assert_eq!(
        cell.load(),
        None,
        "a withdrawn cell must leave a gap, not keep serving a stale reading"
    );
}

/// Withdrawing is not publishing zero. Asserted directly because it is the one
/// substitution that would compile, look reasonable, and quietly invent an
/// operational fact.
#[test]
fn test_withdraw_is_not_a_zero() {
    let withdrawn = Observed::new();
    withdrawn.publish(9);
    withdrawn.withdraw();

    let zeroed = Observed::new();
    zeroed.publish(0);

    assert_eq!(withdrawn.load(), None);
    assert_eq!(zeroed.load(), Some(0));
    assert_ne!(
        withdrawn.load(),
        zeroed.load(),
        "a failed read and a measured zero must not be the same observation"
    );
}

/// A republish after a withdrawal is observed again, so a publisher that
/// recovers is not locked out.
#[test]
fn test_a_recovered_publisher_is_observed_again() {
    let cell = Observed::new();
    cell.withdraw();
    cell.publish(3);
    assert_eq!(cell.load(), Some(3));
}

/// Collection completes against a cell a publisher never wrote, and against one
/// being written concurrently — a callback must never block, and must never see
/// a value its validity flag did not vouch for.
#[test]
fn test_collection_completes_against_a_live_publisher() {
    let cell = Arc::new(Observed::new());
    let publisher = Arc::clone(&cell);

    let writing = thread::spawn(move || {
        for reading in 1..=10_000_u64 {
            publisher.publish(reading);
        }
        publisher.withdraw();
    });

    // The callback's entire job, run against a moving target.
    for _ in 0..10_000 {
        if let Some(reading) = cell.load() {
            assert!(
                reading <= 10_000,
                "a load saw a value no publisher wrote: {reading}"
            );
        }
    }

    writing.join().expect("the publisher thread finished");
    assert_eq!(
        cell.load(),
        None,
        "the publisher's final withdrawal is what the callback observes"
    );
}
