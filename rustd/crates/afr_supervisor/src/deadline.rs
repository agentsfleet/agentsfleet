//! The time left until a wall-clock instant the daemon wrote.
//!
//! A lease's expiry arrives as an instant; the runner waits on a monotonic
//! [`tokio::time::Instant`]. This is the one crossing between the two.

use std::time::Duration;

use jiff::Timestamp;

/// How long from `now` until `then`; zero once `then` has passed.
pub(crate) fn left(now: Timestamp, then: Timestamp) -> Duration {
    afd_core::clock::elapsed(now, then)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use jiff::Timestamp;

    use super::left;

    const NOW: Timestamp = Timestamp::constant(1_760_000_000, 0);

    #[test]
    fn the_time_left_is_the_span_to_the_deadline() {
        let then = afd_core::clock::after(NOW, Duration::from_millis(1_500));
        assert_eq!(left(NOW, then), Duration::from_millis(1_500));
        assert_eq!(left(then, NOW), Duration::ZERO);
    }
}
