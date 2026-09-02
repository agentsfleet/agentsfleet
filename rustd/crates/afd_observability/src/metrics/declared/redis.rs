//! What the Redis connection pool is doing.
//!
//! Declared because the census declares them, and produced by nothing in this
//! daemon: the Rust client holds one multiplexed connection where the Zig
//! daemon hand-rolled a pool, so there is no pool state to read. Every family
//! here is in [`super::super::produced::UNPRODUCED`] with that reason, and the
//! orphan test reads both sides — a family that stops being declared here, or
//! stops being excused there, fails.

use crate::metrics::family::{Declared, CounterKind, GaugeKind};

/// Pool utilisation (leased).
pub const REDIS_POOL_ACTIVE: Declared<GaugeKind> =
    Declared::new("agentsfleet_redis_pool_active");

/// Pool utilisation (ready).
pub const REDIS_POOL_IDLE: Declared<GaugeKind> =
    Declared::new("agentsfleet_redis_pool_idle");

/// Dial volume.
pub const REDIS_POOL_DIALS_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_redis_pool_dials_total");

/// Burst dialing past `max_idle`.
pub const REDIS_POOL_OVERFLOW_DIALS_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_redis_pool_overflow_dials_total");

/// Transport churn: in-flight transport errors.
pub const REDIS_POOL_POISONED_CONNECTIONS_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_redis_pool_poisoned_connections_total");

/// Transport churn: retry-layer redials.
pub const REDIS_POOL_RECONNECTS_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_redis_pool_reconnects_total");

/// Transport churn: over-cap releases.
pub const REDIS_POOL_FORCED_CLOSES_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_redis_pool_forced_closes_total");

/// Currently always 0; acquires never block.
pub const REDIS_POOL_ACQUIRE_TIMEOUTS_TOTAL: Declared<CounterKind> =
    Declared::new("agentsfleet_redis_pool_acquire_timeouts_total");
