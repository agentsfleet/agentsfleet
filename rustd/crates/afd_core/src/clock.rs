//! Wall-clock time, as a seam rather than a global.
//!
//! Mirrors `src/lib/common/clock.zig`, which is the Zig daemon's answer to the
//! same problem, and keeps the two binaries agreeing on what an instant IS: a
//! signed count of milliseconds since the Unix epoch. That is not a storage
//! preference. Every timestamp column in `schema/` is `BIGINT`, every timestamp
//! field in [`afd_wire`] is `i64`, and a `UUIDv7` carries a 48-bit big-endian
//! millisecond field in its own layout — so epoch-milliseconds is already the
//! type three separate contracts are written in.
//!
//! [`afd_wire`]: https://docs.rs/afd_wire
//!
//! # Why there is no monotonic clock here
//!
//! `clock.zig` exposes `nowMonotonicMillis` beside `nowMillis`, and both are
//! `i64` — which means nothing stops a caller subtracting one from the other
//! and getting a number that means nothing. Rust can refuse that outright:
//! elapsed time is [`std::time::Instant`], which has no epoch, no
//! serialization, and no way to become an `i64`. The one Zig caller of the
//! monotonic clock is a deadline loop (`credentials/broker_flight.zig`), and a
//! deadline in this workspace is `tokio::time::timeout` at the call site
//! (Invariant 4). So the monotonic half is not ported: it is replaced by types
//! that already exist, and leaving it out is what makes the mistake unwritable.
//!
//! # Why a clock lives in a crate that claims to do no input/output
//!
//! For the reason [`crate::env`] does: the alternative is worse. A direct
//! `SystemTime::now()` at each call site is a global read that no test can
//! steer, and the sites that need steering — a cache TTL, an expiry check, a
//! freshness window — are exactly the ones whose failure is invisible until a
//! token is honoured an hour after it expired. Reading a clock pulls in no
//! dependency and starts no runtime, which is what
//! `test_core_dependency_freeze` actually asserts.
//!
//! # How to use it
//!
//! Prefer the parameter to the trait. The Zig daemon's eight production
//! `nowSeconds` callers all do the same thing — read the clock at the edge and
//! hand the value to a pure function (`isTimestampFreshAt`, `verifyAt`,
//! `processAt(request, now_s, now_ms)`) — and that shape needs no seam at all,
//! because the decision under test takes the instant as an argument. Reach for
//! [`Clock`] only where a long-lived owner reads the clock repeatedly and
//! threading a parameter through every call would be worse than injecting the
//! source once: a JWKS cache deciding whether its entry is stale, a sweeper
//! deciding which leases have expired.

use std::time::{SystemTime, UNIX_EPOCH};

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
/// Signed, and negative values are representable, because the Zig daemon's
/// reading is signed and a host whose clock is set before 1970 must produce the
/// SAME number in both binaries — see [`now`].
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
    /// windows are seconds on the wire. Truncating (rather than flooring) is
    /// what `clock.zig`'s `nowSeconds` does — `@divTrunc`, not `@divFloor` —
    /// and the two disagree for pre-epoch values, which is precisely where a
    /// silent divergence between the binaries would hide.
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
/// Returns a NEGATIVE reading, matching `clock.zig`: its `nowNanos` builds
/// `ts.sec * ns_per_s + ts.nsec` straight from `clock_gettime`, so a pre-epoch
/// host yields a negative number there too. `SystemTime::duration_since` calls
/// that an error and hands back the magnitude, so the sign is restored here.
///
/// The obvious alternative — map the error to `0` — is the one thing this must
/// not do, and `clock.zig` says why in its own words: *"a silent epoch-0 return
/// would corrupt `UUIDv7` timestamp ordering (the ids stay unique, but stop
/// sorting by mint time)"*. Two hosts, one with a broken clock, would mint ids
/// that interleave wrongly and rows that claim to predate the epoch by
/// different amounts. A wrong-but-consistent answer is recoverable; two
/// binaries disagreeing about the same broken host is not.
#[must_use]
pub fn now() -> UnixMillis {
    millis_at(SystemTime::now())
}

/// The same conversion, over an instant the caller supplies.
///
/// PURE — it reads no clock, which is what lets the pre-epoch branch be proven
/// at all. A host clock cannot be set before 1970 on demand from inside a test,
/// and that branch is precisely the one carrying a parity claim against the Zig
/// daemon, so leaving it unreachable would mean the claim was never checked.
/// The shape is the daemon's own: `isTimestampFreshAt` beside
/// `isTimestampFresh`, the decision taken as an argument and the clock read by
/// a one-line wrapper.
#[must_use]
pub fn millis_at(instant: SystemTime) -> UnixMillis {
    let millis = match instant.duration_since(UNIX_EPOCH) {
        Ok(elapsed) => i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX),
        // Pre-epoch: `SystemTimeError` carries how far BEFORE the epoch it is,
        // as a positive magnitude, so the sign is put back here.
        Err(before) => {
            i64::try_from(before.duration().as_millis()).map_or(i64::MIN, i64::saturating_neg)
        }
    };
    UnixMillis::from_millis(millis)
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
            .fetch_update(
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
