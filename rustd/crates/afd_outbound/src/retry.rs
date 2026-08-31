//! How many times an answer is offered to a vendor, and how long between.
//!
//! # The whole schedule is `backon`'s, not this crate's
//!
//! `afd_fleet_lifecycle::install` writes a `Schedule` iterator adapter so
//! `backon` drives its loop from `afd_redis::Backoff`'s delays, and that is
//! right THERE: those delays are shared with the subscription hub's reconnect,
//! so the install and the pump recover from one Redis outage on one proven
//! curve. Nothing shares this curve. A vendor POST retry has no sibling to
//! agree with, so the adapter would buy nothing and cost a jitter source, an
//! `Iterator` impl and its own tests.
//!
//! [`ExponentialBuilder`] already carries every part of it: the factor, the
//! floor, the ceiling, the attempt count, and `with_jitter`. Writing those
//! five numbers is the whole module.
//!
//! # Why jitter is an improvement rather than parity
//!
//! `worker.zig` sleeps a flat `200ms << attempt`. Every worker that saw the
//! same vendor outage therefore retries in the same millisecond, and the
//! recovering vendor is hit by the whole deployment at once — the retry storm
//! that keeps it down. `backon` adds a random offset inside each delay.
//! Dimension 5.1's "jittered" is this departure from the port, not a copy of
//! it.
//!
//! # The budget is small on purpose
//!
//! Three attempts, two sleeps, under a second of waiting. A longer budget does
//! not buy reliability here, because the durable stream already provides it: a
//! job this worker never acknowledges is redelivered to the next process. What
//! the inline retry covers is the narrow case that budget suits — a 429, a
//! brief 5xx — and holding the worker on one job for a minute would stall every
//! answer behind it, because delivery is serial.

use std::time::Duration;

use backon::ExponentialBuilder;

/// How many times one answer is offered before it is given up on.
///
/// `worker.zig`'s `MAX_ATTEMPTS` is also three, for the reason its comment
/// gives: a crash is already covered by pending redelivery, so this budget only
/// has to cover a transient.
pub const DELIVERY_ATTEMPTS: usize = 3;

/// The first delay, and the base the rest double from. The Zig's own base.
const FIRST_DELAY: Duration = Duration::from_millis(200);

/// The ceiling one delay may reach.
///
/// Two sleeps come to 600ms before jitter and under a second with it. Delivery
/// is serial, so this wait is paid by every answer queued behind this one.
const MAX_DELAY: Duration = Duration::from_millis(800);

/// The schedule one delivery's attempts are spread over.
///
/// `max_times` is the number of RETRIES, so it is one fewer than the attempts:
/// the first try is not a retry. Deriving it from [`DELIVERY_ATTEMPTS`] rather
/// than writing `2` is what keeps the two from drifting — the Zig's install
/// wrote its sleeps down, derived the count with `attempt + 1 >= len`, and
/// shipped a loop guard that left the last delay unreachable while the comment
/// beside it promised four tries.
#[must_use]
pub const fn delivery_schedule() -> ExponentialBuilder {
    ExponentialBuilder::new()
        .with_min_delay(FIRST_DELAY)
        .with_max_delay(MAX_DELAY)
        .with_max_times(DELIVERY_ATTEMPTS - 1)
        .with_jitter()
}
