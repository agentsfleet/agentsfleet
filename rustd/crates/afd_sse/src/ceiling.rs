//! How many event streams this instance will carry at once.
//!
//! # Why streams are not counted against the request ceiling
//!
//! `afd_api`'s admission layer sheds by IN-FLIGHT REQUESTS, and it is right to:
//! an ordinary request is in flight for milliseconds, so the count tracks load.
//! A stream is in flight for as long as somebody has a tab open. Counted
//! together, one wall of dashboards would hold every admission slot and the API
//! would answer nothing — so streams get a ceiling of their own, and a client
//! that meets it is told to reopen the STREAM rather than to slow every request
//! down.
//!
//! # A permit, not a registry
//!
//! The daemon this ports keeps a map of live streams so a shutdown can reach
//! into each one and close its socket. Nothing here needs that: a stream is a
//! task, its subscription is owned by the future serving it, and both go away
//! when the connection does — so the slot is released by `Drop` and cannot be
//! leaked by a path that forgot to deregister. Draining is
//! [`SubscriptionHub::shutdown`], which closes what every live stream is
//! waiting on; this type's [`close`] only stops NEW ones from starting.
//!
//! [`SubscriptionHub::shutdown`]: afd_redis::SubscriptionHub::shutdown
//! [`close`]: Ceiling::close

use std::sync::Arc;

use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// The instance's concurrent-stream budget.
#[derive(Debug, Clone)]
pub struct Ceiling {
    slots: Arc<Semaphore>,
    capacity: usize,
}

impl Ceiling {
    /// A ceiling admitting `capacity` streams at once.
    ///
    /// Clamped to what a semaphore can hold. A configured value past that is a
    /// knob somebody typed wrong, and refusing to boot over it would be worse
    /// than serving the largest ceiling this process can express.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        let capacity = capacity.min(Semaphore::MAX_PERMITS);
        Self {
            slots: Arc::new(Semaphore::new(capacity)),
            capacity,
        }
    }

    /// A slot for one stream, or `None` when the instance is full or draining.
    ///
    /// Non-blocking by design: a caller at the ceiling must be REFUSED, not
    /// queued. Queuing would hold the request open until a stranger closed
    /// their tab, which is indistinguishable to a client from a hang.
    #[must_use]
    pub fn admit(&self) -> Option<Slot> {
        Arc::clone(&self.slots)
            .try_acquire_owned()
            .ok()
            .map(Slot::new)
    }

    /// How many streams this instance is carrying.
    #[must_use]
    pub fn live(&self) -> usize {
        self.capacity.saturating_sub(self.slots.available_permits())
    }

    /// How many it will carry.
    #[must_use]
    pub const fn capacity(&self) -> usize {
        self.capacity
    }

    /// Stops admitting, leaving the streams already running to finish.
    ///
    /// The shutdown order this belongs to: stop taking new streams, then close
    /// the hub, which is what actually ends the ones in flight.
    pub fn close(&self) {
        self.slots.close();
    }
}

/// One stream's claim on the instance, released when the stream ends.
#[derive(Debug)]
pub struct Slot {
    /// Held for its `Drop`, never read. The permit IS the claim.
    _permit: OwnedSemaphorePermit,
}

impl Slot {
    /// Wraps the permit that is the claim.
    const fn new(permit: OwnedSemaphorePermit) -> Self {
        Self { _permit: permit }
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]

    use super::Ceiling;

    /// A fresh ceiling is carrying nothing and admits up to its capacity.
    #[test]
    fn should_admit_up_to_its_capacity() {
        let ceiling = Ceiling::new(2);
        assert_eq!(ceiling.live(), 0);

        let first = ceiling.admit();
        let second = ceiling.admit();
        assert!(first.is_some() && second.is_some());
        assert_eq!(ceiling.live(), 2);
    }

    /// The stream past the ceiling is refused rather than queued.
    #[test]
    fn should_refuse_the_stream_past_the_ceiling() {
        let ceiling = Ceiling::new(1);
        let held = ceiling.admit();
        assert!(held.is_some());
        assert!(
            ceiling.admit().is_none(),
            "a caller at the ceiling is refused, never parked"
        );
    }

    /// A finished stream gives its slot back, with no deregister call.
    ///
    /// The property the permit exists for: a handler that returns early, panics
    /// or is cancelled still releases, because the release is `Drop`.
    #[test]
    fn should_release_the_slot_when_the_stream_ends() {
        let ceiling = Ceiling::new(1);
        let held = ceiling.admit().expect("the first stream is admitted");
        assert_eq!(ceiling.live(), 1);
        drop(held);
        assert_eq!(ceiling.live(), 0);
        assert!(ceiling.admit().is_some(), "the slot came back");
    }

    /// A ceiling of zero serves no streams at all.
    ///
    /// The knob an operator sets to turn the surface off, and it must not read
    /// as "unlimited" — every other ceiling in this daemon counts UP to its
    /// value, so zero meaning "none" is the only reading that composes.
    #[test]
    fn should_serve_nothing_at_a_ceiling_of_zero() {
        let ceiling = Ceiling::new(0);
        assert!(ceiling.admit().is_none());
        assert_eq!(ceiling.capacity(), 0);
    }

    /// A draining instance refuses new streams.
    #[test]
    fn should_refuse_new_streams_once_draining() {
        let ceiling = Ceiling::new(4);
        let held = ceiling.admit().expect("an open instance admits");
        ceiling.close();
        assert!(ceiling.admit().is_none(), "a draining instance takes none");
        drop(held);
        assert!(ceiling.admit().is_none(), "and it stays closed");
    }
}
