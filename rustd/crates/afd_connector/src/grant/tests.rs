//! The connect stamp's monotonicity, which the broker's cache fingerprint
//! depends on.
#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use jiff::Timestamp;

use super::connected_at;

/// The instant `millis` after the epoch: fixtures here are spelled in the
/// milliseconds the stored shape carries.
fn at(millis: i64) -> Timestamp {
    Timestamp::from_millisecond(millis).expect("the fixture instant is in range")
}

/// Two stamps from one instant are still strictly increasing.
///
/// The property the broker's cache fingerprint depends on: without it, a
/// reconnect to a different account inside one millisecond keeps serving
/// the previous account's cached token — see [`super::LAST_CONNECT_STAMP`].
#[test]
fn two_connects_in_one_millisecond_do_not_share_a_stamp() {
    let instant = at(1_700_000_000_000);

    let first = connected_at(instant);
    let second = connected_at(instant);

    assert!(
        second.as_millisecond() > first.as_millisecond(),
        "{} must follow {}",
        second.as_millisecond(),
        first.as_millisecond(),
    );
}

/// A later wall clock wins over the counter, rather than trailing it.
///
/// The stamp is a real instant that happens to be monotonic, not a
/// sequence: a handle whose `connected_at_ms` drifted away from the clock
/// would misreport when a workspace connected.
#[test]
fn a_later_instant_is_taken_rather_than_the_previous_stamp_plus_one() {
    let early = connected_at(at(1_700_000_000_000));
    let far_later = 1_800_000_000_000;

    let late = connected_at(at(far_later));

    assert!(late.as_millisecond() > early.as_millisecond());
    assert_eq!(late.as_millisecond(), far_later);
}

/// Connects racing on every core never share a stamp.
///
/// Sixteen threads released together by a barrier, each taking a thousand
/// stamps for one instant: the exchange loses to a sibling constantly, and a
/// loop that kept its stale read instead of the winner's value would hand
/// two connects the same stamp. Every stamp distinct is the whole property.
#[test]
fn connects_racing_on_every_core_never_share_a_stamp() {
    const THREADS: usize = 16;
    const EACH: usize = 1_000;
    let instant = at(1_700_000_000_000);
    let start = std::sync::Arc::new(std::sync::Barrier::new(THREADS));

    let handles: Vec<_> = (0..THREADS)
        .map(|_| {
            let start = std::sync::Arc::clone(&start);
            std::thread::spawn(move || {
                start.wait();
                (0..EACH)
                    .map(|_| connected_at(instant).as_millisecond())
                    .collect::<Vec<_>>()
            })
        })
        .collect();
    let mut stamps: Vec<i64> = handles
        .into_iter()
        .flat_map(|handle| handle.join().expect("a stamping thread finishes"))
        .collect();

    let taken = stamps.len();
    stamps.sort_unstable();
    stamps.dedup();
    assert_eq!(
        stamps.len(),
        taken,
        "every racing connect got its own stamp"
    );
}
