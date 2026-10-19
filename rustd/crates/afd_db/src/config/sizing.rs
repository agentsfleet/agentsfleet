//! How large a role's pool is, how much of it is warm, and how long an acquire
//! and a handshake may take, when the environment says nothing.
//!
//! Split from [`super`] for length: these are the defaults the knobs there
//! override, and each carries the measurement that chose it.

use std::time::Duration;

/// The pool's ceiling, sized the way `core_api` sizes its own.
///
/// It used to be the API's in-flight ceiling over a "per-connection
/// request-sharing factor" — 256 / 64. The 256 is real and defined elsewhere;
/// the 64 was asserted and never derived, and four connections is what it
/// produced.
///
/// Four is too few, and the failure it causes is not a slow request. sqlx opens
/// connections lazily, so a burst against a cold pool queues behind connection
/// ESTABLISHMENT — measured on this lane's Postgres at 147 ms typically and
/// 337 ms under load — and blows the two-second acquire budget while Postgres
/// sits healthy. `Db::acquire` then reads a pool below its ceiling as evidence
/// the datastore is absent and answers 503.
///
/// The ceiling is a share of the DATABASE's capacity, not the host's.
///
/// A connection costs a Postgres backend process, and how many the server can
/// serve has nothing to do with how many cores this replica has. Sizing from
/// `available_parallelism` produced a per-replica fifty, and a stock Postgres
/// admits a hundred connections with three reserved for superusers — so two
/// replicas at fifty exhaust the ordinary slots before a migration, an
/// operator session, or any other service has asked for one.
///
/// So the default divides a budget by the replicas expected to share it. Both
/// numbers are deliberately conservative, because being wrong low costs queuing
/// on one replica and being wrong high costs every client of that database at
/// once — including the ones that would have to be running for anyone to
/// notice.
///
/// A deployment that knows its own budget sets `DATABASE_POOL_SIZE`
/// (role-suffixable as `DATABASE_POOL_SIZE_API`), which is where a real number
/// belongs: it travels with the Postgres `max_connections` the deployment
/// actually runs, and that is not a fact this file can know.
pub(super) fn pool_size_default() -> u32 {
    /// Ordinary slots this service may take of a stock hundred-connection
    /// Postgres, after the three superuser reservations and a margin for the
    /// migrator, operator sessions, and monitoring.
    const SERVICE_CONNECTION_BUDGET: u32 = 80;

    /// Replicas expected to share that budget.
    const EXPECTED_REPLICAS: u32 = 4;

    SERVICE_CONNECTION_BUDGET / EXPECTED_REPLICAS
}

/// How much of the pool is established BEFORE a request needs it.
///
/// The knob that stops a connection being opened in a request's path. Without
/// it the pool starts at zero — `connect_lazy_with` — and the first traffic
/// after boot pays an establishment measured at 147-337 ms inside an acquire
/// budget sized for a wait, not a handshake.
///
/// **This floor is established by [`crate::Db::warm`], not by sqlx.** Passing
/// `min_connections` to the builder is necessary and not sufficient: sqlx only
/// bootstraps the floor from zero when BOTH `max_lifetime` and `idle_timeout`
/// are `None` (`pool/inner.rs`, the `(None, None)` arm), and `pool.rs` pins
/// both. Every other arm reaches the floor through the idle reaper, whose body
/// is `for _ in 0..num_idle()` — zero on a pool that has never opened a
/// connection, so the body never runs and the floor is never approached. The
/// setting still earns its place: once connections exist, the reaper does keep
/// the pool from falling below it.
///
/// A quarter of the ceiling rather than all of it: connections held open cost
/// Postgres slots whether or not traffic exists, and the point is to remove the
/// cold start, not to reserve the whole pool against a burst that may not come.
/// The right number is a measured steady-state concurrency, which this file
/// cannot know; `DATABASE_MIN_POOL_SIZE` is where a measured one goes.
pub(super) fn min_connections_default(max_connections: u32) -> u32 {
    const WARM_FRACTION: u32 = 4;
    (max_connections / WARM_FRACTION).max(1)
}

/// How long an acquire may take, INCLUDING opening a connection if the pool
/// has to.
///
/// This was two seconds, on the reasoning that a starved pool should fail fast
/// rather than read as a slow request. That reasoning is sound and the number
/// was still wrong, because it is not only the wait: sqlx spends ONE deadline
/// on both halves. `PoolInner::acquire` sets `deadline = now + acquire_timeout`
/// and hands that same deadline to `connect()`
/// (`sqlx-core-0.9.0/src/pool/inner.rs`), and upstream's own `Debug` impl
/// prints the field as `connect_timeout` (`options.rs:590`). There is no
/// second knob.
///
/// So a budget below the handshake cost is a pool that can never grow. Every
/// attempt to open a connection expires, [`crate::Db::warm`] reaches zero, the
/// pool stays empty, and every request pays — and loses — the same race. A
/// probe against the development database through the production pool showed
/// exactly that: the boot handshake took 2.7 s, warm established 0 of 2 inside
/// a 10 s deadline, and all eight callers failed their first acquire with the
/// pool below its ceiling. Identical on the direct port and through
/// `PgBouncer`,
/// which is what rules the pooler out as the cause.
///
/// Five seconds covers a cross-region TLS handshake with headroom while still
/// failing inside any sensible gateway timeout. A saturated pool now makes a
/// caller wait longer before its 503, which is the trade: an answer at five
/// seconds beats a refusal at two when the connection would have opened.
///
/// A deployment whose database is in the same region can lower it with
/// `DATABASE_ACQUIRE_TIMEOUT_MS`. Do not lower it below the p99 establishment
/// that deployment actually measures.
pub(super) const ACQUIRE_TIMEOUT_DEFAULT: Duration = Duration::from_secs(5);

/// The handshake budget for the BOOT PROBE, which is the one place this crate
/// can bound a handshake on its own.
///
/// Not the pool's: see [`ACQUIRE_TIMEOUT_DEFAULT`] for why the pool cannot
/// be given a separate one. Not tunable per role.
pub(super) const CONNECT_TIMEOUT_DEFAULT: Duration = Duration::from_secs(10);
