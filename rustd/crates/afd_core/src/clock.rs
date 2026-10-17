//! Wall-clock time, as a seam rather than a global.
//!
//! An instant is a [`jiff::Timestamp`] holding whole milliseconds. Postgres
//! stores it as `TIMESTAMPTZ(3)`, the API writes it as RFC 3339 in UTC, and a
//! `UUIDv7` carries the same millisecond count in its own layout, so a value
//! read here survives every one of those round trips unchanged. [`now`] cuts
//! the reading to the millisecond for exactly that reason: a sub-millisecond
//! instant written to the database would come back as a different value.
//!
//! # Why there is no monotonic clock here
//!
//! Elapsed time is [`std::time::Instant`], which has no epoch, no serialization
//! and no way to become a timestamp, so it cannot be compared with a wall-clock
//! reading by mistake. A deadline in this workspace is `tokio::time::timeout` at
//! the call site (Invariant 4). Types that already exist cover the monotonic
//! half, and leaving it out is what makes the mistake unwritable.
//!
//! # Why a clock lives in a crate that claims to do no input/output
//!
//! For the reason [`crate::env`] does: the alternative is worse. A direct
//! `SystemTime::now()` at each call site is a global read that no test can
//! steer, and the sites that need steering — a cache TTL, an expiry check, a
//! freshness window — are exactly the ones whose failure is invisible until a
//! token is honoured an hour after it expired. Reading a clock pulls in no
//! runtime.
//!
//! # How to use it
//!
//! Prefer the parameter to the trait. Most callers read the clock once at the
//! edge and hand the value to a pure function — the shape [`instant_at`] has
//! beside [`now`] — and that needs no seam at all, because the decision under
//! test takes the instant as an argument. Reach for [`Clock`] only where a
//! long-lived owner reads the clock repeatedly: a JWKS cache deciding whether
//! its entry is stale, a sweeper deciding which leases have expired.
//!
//! A timestamp a statement stores is not read here at all. Lifecycle stamps
//! are the database's `now()` and deadlines its `clock_timestamp()`
//! (`.orly/docs/SCHEMA_CONVENTIONS.md` §Timestamps); this clock is for
//! decisions the daemon makes in memory.

use std::time::{Duration, SystemTime};

use jiff::{RoundMode, Timestamp, TimestampRound, Unit};

/// The current wall-clock instant, in whole milliseconds.
#[must_use]
#[expect(
    clippy::disallowed_methods,
    reason = "this is the owner: the one wall-clock read every caller routes through"
)]
pub fn now() -> Timestamp {
    instant_at(SystemTime::now())
}

/// The same conversion, over an instant the caller supplies.
///
/// PURE — it reads no clock, which is what lets the pre-epoch and out-of-range
/// branches be proven at all: a host clock cannot be set before 1970 on demand
/// from inside a test.
///
/// A clock set before 1970 reads as the pre-epoch instant it is, never as the
/// epoch: a silent epoch-0 reading would hide a broken clock behind a real
/// instant and stop `UUIDv7`s sorting by mint time. A reading past the range
/// jiff represents (years -9999 to 9999) saturates to that end of the range.
#[must_use]
pub fn instant_at(instant: SystemTime) -> Timestamp {
    let reading = Timestamp::try_from(instant).unwrap_or_else(|_out_of_range| {
        if instant < SystemTime::UNIX_EPOCH {
            Timestamp::MIN
        } else {
            Timestamp::MAX
        }
    });
    whole_millis(reading)
}

/// `instant` floored to the millisecond.
///
/// Floored rather than truncated, so a pre-epoch reading moves earlier like
/// every other one does; the two differ only before 1970.
#[must_use]
pub fn whole_millis(instant: Timestamp) -> Timestamp {
    let floor = TimestampRound::new()
        .smallest(Unit::Millisecond)
        .mode(RoundMode::Floor);
    instant.round(floor).unwrap_or(instant)
}

/// `instant` moved forward by `span`, saturating at the end of the range.
///
/// Saturating rather than wrapping: a TTL added to a clock near the end of the
/// range is a broken input, and wrapping would turn "far future" into "long
/// past", an expiry check that then passes. jiff answers a `Duration` with a
/// `Result` only because its calendar spans can fail; a `Duration` cannot, so
/// the fallback is the saturation point itself.
#[must_use]
pub fn after(instant: Timestamp, span: Duration) -> Timestamp {
    instant.saturating_add(span).unwrap_or(Timestamp::MAX)
}

/// `instant` moved back by `span`, saturating at the start of the range.
#[must_use]
pub fn before(instant: Timestamp, span: Duration) -> Timestamp {
    instant.saturating_sub(span).unwrap_or(Timestamp::MIN)
}

/// A span in whole milliseconds, saturated rather than wrapped, for the wire
/// fields that spell a span as a count (`retry_after_ms`, a Redis `PX`).
#[must_use]
pub fn saturating_millis(span: Duration) -> u64 {
    u64::try_from(span.as_millis()).unwrap_or(u64::MAX)
}

/// A span in whole milliseconds for a field carried signed; saturated rather
/// than wrapped.
#[must_use]
pub fn saturating_millis_signed(span: Duration) -> i64 {
    i64::try_from(span.as_millis()).unwrap_or(i64::MAX)
}

/// A source of the current wall-clock instant.
///
/// Injected only where a long-lived owner reads the clock repeatedly; see the
/// module documentation for why a parameter beats this in every other case.
pub trait Clock: Send + Sync + std::fmt::Debug {
    /// The current instant, in whole milliseconds.
    fn now(&self) -> Timestamp;
}

/// The real clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        now()
    }
}

/// A clock the test drives, for the expiry and staleness decisions a real one
/// cannot be asked to make on demand.
///
/// Clones share one reading, the way `exonum`'s `MockTimeProvider` does: the
/// point of the seam is to hand a copy to the component under test and keep one
/// to move time with, which does not work if the copy has its own clock. Every
/// reading is floored to the millisecond, so it never carries a fraction the
/// real clock could not.
#[cfg(feature = "test-util")]
#[derive(Debug, Clone)]
pub struct FixedClock(std::sync::Arc<std::sync::Mutex<Timestamp>>);

#[cfg(feature = "test-util")]
impl FixedClock {
    /// A clock stopped at `instant`.
    #[must_use]
    pub fn at(instant: Timestamp) -> Self {
        Self(std::sync::Arc::new(std::sync::Mutex::new(whole_millis(
            instant,
        ))))
    }

    /// Moves every clone to `instant`.
    pub fn set(&self, instant: Timestamp) {
        *self.reading() = whole_millis(instant);
    }

    /// Moves every clone forward by `span`, saturating at the end of the range.
    pub fn advance(&self, span: Duration) {
        let mut reading = self.reading();
        *reading = whole_millis(after(*reading, span));
    }

    /// Moves every clone BACK by `span`, saturating at the start of the range.
    ///
    /// A wall clock that goes back is a real event (an operator correcting
    /// drift, an NTP step), and code that treats time as monotonic breaks
    /// exactly there.
    pub fn rewind(&self, span: Duration) {
        let mut reading = self.reading();
        *reading = whole_millis(before(*reading, span));
    }

    /// The shared reading. A poisoned lock still holds a valid instant, so a
    /// test that panicked mid-update does not take every clone down with it.
    fn reading(&self) -> std::sync::MutexGuard<'_, Timestamp> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[cfg(feature = "test-util")]
impl Clock for FixedClock {
    fn now(&self) -> Timestamp {
        *self.reading()
    }
}
