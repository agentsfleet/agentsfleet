//! The time left until a wall-clock instant the daemon wrote.
//!
//! A lease's expiry arrives as an instant; the runner waits on a monotonic
//! [`tokio::time::Instant`]. This is the one crossing between the two.

use std::time::Duration;

use jiff::Timestamp;

/// How long from `now` until `then`; zero once `then` has passed.
///
/// Zero rather than an error: a deadline already behind the clock is one the
/// caller gives up at once, which is exactly what a zero wait does.
pub(crate) fn left(now: Timestamp, then: Timestamp) -> Duration {
    Duration::try_from(then.duration_since(now)).unwrap_or(Duration::ZERO)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use jiff::Timestamp;

    use super::left;

    const NOW: Timestamp = Timestamp::constant(1_760_000_000, 0);

    #[test]
    fn the_span_to_a_later_instant_is_its_distance() {
        let then = afd_core::clock::after(NOW, Duration::from_millis(1_500));
        assert_eq!(left(NOW, then), Duration::from_millis(1_500));
    }

    #[test]
    fn an_instant_already_passed_leaves_nothing() {
        let then = afd_core::clock::before(NOW, Duration::from_secs(1));
        assert_eq!(left(NOW, then), Duration::ZERO);
        assert_eq!(left(NOW, NOW), Duration::ZERO);
    }

    /// jiff's whole range, about twenty thousand years, still fits a
    /// `Duration`: the widest span is read as itself, never as zero.
    #[test]
    fn the_widest_span_does_not_wrap() {
        const NINETEEN_THOUSAND_YEARS: Duration =
            afd_core::timing::DAY.saturating_mul(365 * 19_000);
        assert!(left(Timestamp::MIN, Timestamp::MAX) > NINETEEN_THOUSAND_YEARS);
    }
}
