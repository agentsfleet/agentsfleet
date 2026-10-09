//! Wall-clock time, as a seam rather than a global.
//!
//! An instant IS a signed count of milliseconds since the Unix epoch. That is
//! not a storage preference. Every timestamp column in `schema/` is `BIGINT`,
//! every timestamp field in [`afd_wire`] is `i64`, and a `UUIDv7` carries a
//! 48-bit big-endian millisecond field in its own layout — so epoch-milliseconds
//! is already the type three separate formats are written in.
//!
//! [`afd_wire`]: https://docs.rs/afd_wire
//!
//! # Why there is no monotonic clock here
//!
//! A monotonic reading and a wall-clock reading sharing one integer type lets a
//! caller subtract one from the other and get a number that means nothing. Rust
//! can refuse that outright: elapsed time is [`std::time::Instant`], which has
//! no epoch, no serialization, and no way to become an `i64`. A deadline in
//! this workspace is `tokio::time::timeout` at the call site (Invariant 4). So
//! there is no monotonic half here: types that already exist cover it, and
//! leaving it out is what makes the mistake unwritable.
//!
//! # Why a clock lives in a crate that claims to do no input/output
//!
//! For the reason [`crate::env`] does: the alternative is worse. A direct
//! `SystemTime::now()` at each call site is a global read that no test can
//! steer, and the sites that need steering — a cache TTL, an expiry check, a
//! freshness window — are exactly the ones whose failure is invisible until a
//! token is honoured an hour after it expired. Reading a clock pulls in no
//! dependency and starts no runtime.
//!
//! # How to use it
//!
//! Prefer the parameter to the trait. Most callers read the clock once at the
//! edge and hand the value to a pure function — the shape [`millis_at`] has
//! beside [`now`] — and that needs no seam at all, because the decision under
//! test takes the instant as an argument. Reach for
//! [`Clock`] only where a long-lived owner reads the clock repeatedly and
//! threading a parameter through every call would be worse than injecting the
//! source once: a JWKS cache deciding whether its entry is stale, a sweeper
//! deciding which leases have expired.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Milliseconds in one second, as the divisor a seconds-valued claim needs.
const MILLIS_PER_SECOND: i64 = 1_000;

/// A wall-clock instant, as milliseconds since the Unix epoch.
///
/// A newtype rather than a bare `i64` because the invariant worth keeping is
/// not a range, it is a MEANING: this number is comparable with another
/// wall-clock reading and with a `BIGINT` column, and it is not comparable with
/// an elapsed-time measurement. The wrapper is what makes the second kind of
/// comparison fail to compile instead of failing in production.
///
/// Signed, and negative values are representable, because a host whose clock is
/// set before 1970 must read as the pre-epoch instant it is, never as the epoch
/// — see [`now`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UnixMillis(i64);

impl UnixMillis {
    /// The Unix epoch itself.
    pub const EPOCH: Self = Self(0);

    /// Wraps a millisecond count that already came from a trusted source — a
    /// `BIGINT` column, a wire payload, a fixture.
    #[must_use]
    pub const fn from_millis(millis: i64) -> Self {
        Self(millis)
    }

    /// The millisecond count, for a bind parameter or a wire field.
    #[must_use]
    pub const fn as_millis(self) -> i64 {
        self.0
    }

    /// The same instant in whole seconds, truncated toward zero.
    ///
    /// `exp`, `nbf` and `iat` are seconds in every JWT, and webhook signature
    /// windows are seconds on the wire. Truncation and flooring disagree only
    /// for pre-epoch values, which is precisely where a silent change of
    /// rounding would hide; the pre-epoch rows in `tests/clock.rs` pin it.
    #[must_use]
    pub const fn as_seconds(self) -> i64 {
        self.0 / MILLIS_PER_SECOND
    }

    /// This instant moved forward by `millis`, saturating at the bounds.
    ///
    /// Saturating rather than wrapping: a TTL added to a clock near `i64::MAX`
    /// is a broken input, and wrapping would turn "far future" into "long past"
    /// — an expiry check that then passes.
    #[must_use]
    pub const fn saturating_add_millis(self, millis: i64) -> Self {
        Self(self.0.saturating_add(millis))
    }

    /// Milliseconds from `earlier` to `self`, negative when `self` is earlier.
    ///
    /// Saturating for the same reason as [`Self::saturating_add_millis`].
    #[must_use]
    pub const fn saturating_millis_since(self, earlier: Self) -> i64 {
        self.0.saturating_sub(earlier.0)
    }
}

/// The current wall-clock instant.
///
/// # A clock set before 1970
///
/// Returns a NEGATIVE reading. `SystemTime::duration_since` calls a pre-epoch
/// instant an error and hands back the magnitude, so the sign is restored here.
///
/// The obvious alternative — map the error to `0` — is the one thing this must
/// not do: a silent epoch-0 return would corrupt `UUIDv7` timestamp ordering
/// (the ids stay unique, but stop sorting by mint time). Two hosts, one with a
/// broken clock, would mint ids that interleave wrongly and rows that claim to
/// predate the epoch by different amounts. A negative reading is wrong but
/// honest, and recoverable; an epoch-0 reading hides the broken clock behind a
/// real instant.
#[must_use]
pub fn now() -> UnixMillis {
    millis_at(SystemTime::now())
}

/// The same conversion, over an instant the caller supplies.
///
/// PURE — it reads no clock, which is what lets the pre-epoch branch be proven
/// at all. A host clock cannot be set before 1970 on demand from inside a test,
/// so a branch only the real clock could reach would carry a claim nobody ever
/// checked. The decision takes the instant as an argument and [`now`] is the
/// one-line wrapper that reads the clock.
#[must_use]
pub fn millis_at(instant: SystemTime) -> UnixMillis {
    let millis = match instant.duration_since(UNIX_EPOCH) {
        Ok(elapsed) => saturating_millis_signed(elapsed),
        // Pre-epoch: `SystemTimeError` carries how far BEFORE the epoch it is,
        // as a positive magnitude, so the sign is put back here.
        Err(before) => {
            i64::try_from(before.duration().as_millis()).map_or(i64::MIN, i64::saturating_neg)
        }
    };
    UnixMillis::from_millis(millis)
}

/// A span in whole milliseconds, saturated rather than wrapped.
#[must_use]
pub fn saturating_millis(span: Duration) -> u64 {
    u64::try_from(span.as_millis()).unwrap_or(u64::MAX)
}

/// A span in whole milliseconds for a field carried signed, as epoch
/// arithmetic is; saturated rather than wrapped.
#[must_use]
pub fn saturating_millis_signed(span: Duration) -> i64 {
    i64::try_from(span.as_millis()).unwrap_or(i64::MAX)
}

/// A source of the current wall-clock instant.
///
/// Injected only where a long-lived owner reads the clock repeatedly; see the
/// module documentation for why a parameter beats this in every other case.
pub trait Clock: Send + Sync + std::fmt::Debug {
    /// The current instant.
    fn now(&self) -> UnixMillis;
}

/// The real clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> UnixMillis {
        now()
    }
}

/// A clock the test drives, for the expiry and staleness decisions a real one
/// cannot be asked to make on demand.
///
/// Clones share one reading, the way `exonum`'s `MockTimeProvider` does: the
/// point of the seam is to hand a copy to the component under test and keep one
/// to move time with, which does not work if the copy has its own clock.
#[cfg(feature = "test-util")]
#[derive(Debug, Clone)]
pub struct FixedClock(std::sync::Arc<std::sync::atomic::AtomicI64>);

#[cfg(feature = "test-util")]
impl FixedClock {
    /// A clock stopped at `instant`.
    #[must_use]
    pub fn at(instant: UnixMillis) -> Self {
        Self(std::sync::Arc::new(std::sync::atomic::AtomicI64::new(
            instant.as_millis(),
        )))
    }

    /// Moves every clone to `instant`.
    pub fn set(&self, instant: UnixMillis) {
        self.0
            .store(instant.as_millis(), std::sync::atomic::Ordering::SeqCst);
    }

    /// Moves every clone forward by `millis`, saturating at the bounds.
    ///
    /// Negative values step the clock BACKWARD on purpose: a wall clock that
    /// goes back is a real event (an operator correcting drift, an NTP step),
    /// and code that treats time as monotonic breaks exactly there.
    pub fn advance_millis(&self, millis: i64) {
        let _previous = self
            .0
            .try_update(
                std::sync::atomic::Ordering::SeqCst,
                std::sync::atomic::Ordering::SeqCst,
                |current| Some(current.saturating_add(millis)),
            )
            .unwrap_or_default();
    }
}

#[cfg(feature = "test-util")]
impl Clock for FixedClock {
    fn now(&self) -> UnixMillis {
        UnixMillis::from_millis(self.0.load(std::sync::atomic::Ordering::SeqCst))
    }
}
