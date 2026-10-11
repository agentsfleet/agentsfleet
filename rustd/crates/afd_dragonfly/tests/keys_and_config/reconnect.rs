//! The reconnect schedule every replica redials a lost Dragonfly on.
//!
//! Split from the parent for length; the key shapes stay there.

use std::time::Duration;

use afd_dragonfly::production_backoff;
use backon::BackoffBuilder as _;

/// The ceiling the reconnect schedule must never exceed.
///
/// Five seconds, the value `production_backoff` sets. Spelled here as the
/// PROMISE rather than read from the source, so a change to the schedule has to
/// change this line too and be seen: how long an outage takes to recover from
/// is an operational number, not an implementation detail.
const BACKOFF_CAP: Duration = Duration::from_secs(5);

/// The reconnect schedule grows, stops growing, and never waits forever.
///
/// `backon` owns the arithmetic, so what is asserted here is our CONFIGURATION
/// of it — the two knobs an operator feels. The ceiling is the one that matters:
/// a backoff that keeps doubling turns a ten-minute Dragonfly outage into an
/// hour-long one, because the last sleep started before Dragonfly came back.
///
/// Jitter is on in production, so each delay is a random offset inside its
/// step rather than a fixed number. The assertions are therefore bounds, which
/// is what the property actually is — an equality here would be asserting that
/// the spread is absent.
#[test]
fn test_the_reconnect_schedule_grows_then_settles_at_its_ceiling() {
    let delays: Vec<Duration> = production_backoff().build().take(12).collect();

    // Jitter ADDS inside the step, so the first delay lands in [200, 400) —
    // asserting equality with the floor would be asserting the spread away.
    assert!(
        delays[0] >= Duration::from_millis(200) && delays[0] < Duration::from_millis(400),
        "the first redial waits one step plus its spread: {:?}",
        delays[0]
    );
    for delay in &delays {
        // The cap bounds the STEP and the jitter is added to it, so the wait
        // itself tops out at twice the ceiling.
        assert!(*delay < BACKOFF_CAP * 2, "{delay:?} past the cap");
    }
    let settled = delays
        .last()
        .copied()
        .expect("a schedule with no attempt limit yields twelve delays");
    assert!(
        settled > Duration::from_secs(1),
        "the schedule must climb to its ceiling rather than staying at the \
         floor: {settled:?}"
    );
}

/// Jitter spreads the delay, so two processes that lost the same Dragonfly do not
/// redial in the same millisecond.
///
/// The reconnect storm is what keeps a struggling Dragonfly down, and a schedule
/// that produced one sequence for everybody would cause it.
#[test]
fn test_the_reconnect_schedule_is_spread() {
    let one: Vec<Duration> = production_backoff().build().take(8).collect();
    let other: Vec<Duration> = production_backoff().build().take(8).collect();

    assert_ne!(one, other);
}
