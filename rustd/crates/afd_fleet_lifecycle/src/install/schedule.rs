//! The adapter between the workspace's backoff schedule and `backon`'s loop.
//!
//! `backon` owns the retry loop — the counter, and the rule that there is no
//! sleep after the final attempt — because that is where the Zig shipped its
//! bug: `attempt + 1 >= len` left the last delay unreachable while the comment
//! beside it promised four tries. `afd_redis::Backoff` keeps owning the DELAYS,
//! because those are the workspace's own and are already proven on the
//! subscription hub's reconnect.
//!
//! This is the twenty lines that let each own its half.

use std::time::Duration;

use super::{STREAM_ATTEMPTS, STREAM_BACKOFF};

/// This install's delays, as the iterator `backon` drives its loop from.
///
/// The adapter between the workspace's schedule and the library's loop: it owns
/// the attempt counter so `Backoff::delay` keeps taking one, and it ENDS after
/// [`STREAM_ATTEMPTS`] − 1 delays, which is what makes four attempts three
/// sleeps. A sleep after the final try would spend 1.5 seconds answering
/// nothing.
pub(super) struct Schedule {
    attempt: u32,
    jitter: u64,
}

impl Schedule {
    /// The schedule for one install, spread by `jitter`.
    pub(super) const fn new(jitter: u64) -> Self {
        Self { attempt: 0, jitter }
    }
}

impl Iterator for Schedule {
    type Item = Duration;

    fn next(&mut self) -> Option<Duration> {
        // One fewer delay than attempts: `backon` sleeps BETWEEN tries, and
        // ending the iterator is how the last one is told to stop.
        (self.attempt + 1 < STREAM_ATTEMPTS).then(|| {
            let delay = STREAM_BACKOFF.delay(self.attempt, self.jitter);
            self.attempt += 1;
            delay
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{STREAM_ATTEMPTS, Schedule};

    #[test]
    fn the_schedule_yields_one_fewer_delay_than_there_are_attempts() {
        // Four attempts, three sleeps. `backon` sleeps BETWEEN tries, so ending
        // the iterator is how the last attempt is told not to wait — a sleep
        // there would spend 1.5 seconds answering nothing.
        let delays: Vec<_> = Schedule::new(0).collect();

        assert_eq!(delays.len(), (STREAM_ATTEMPTS - 1) as usize);
    }

    #[test]
    fn the_delays_grow_and_stay_inside_the_documented_wall_budget() {
        let delays: Vec<_> = Schedule::new(0).collect();

        assert!(
            delays.is_sorted_by(|earlier, later| earlier < later),
            "a schedule that did not grow would hammer a struggling Redis"
        );
        let total: std::time::Duration = delays.iter().sum();
        assert!(total <= std::time::Duration::from_millis(2100), "{total:?}");
    }

    #[test]
    fn two_installs_drawing_different_jitter_do_not_retry_in_step() {
        // Lockstep retries against a struggling Redis are the reconnect storm
        // the spread exists to prevent.
        let one: Vec<_> = Schedule::new(0).collect();
        let other: Vec<_> = Schedule::new(u64::MAX).collect();

        assert_ne!(one, other);
    }
}
