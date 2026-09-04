//! Which database a role connects to, and with what limits.
//!
//! Every knob name here is the Zig daemon's, spelled identically
//! (the retired daemon's `db/pool.zig`): a deployment moves between the two
//! binaries without touching its environment, or the port is not a port.
//!
//! The TLS posture a URL resolves to lives in [`tls`], with the history that
//! explains it.

use std::time::Duration;

use sqlx::postgres::PgConnectOptions;

mod tls;

use crate::error::{Error, ErrorKind, Result};
use afd_core::env::EnvSource;

/// The pool's ceiling, sized the way `core_api` sizes its own.
///
/// It used to be the API's in-flight ceiling over a "per-connection
/// request-sharing factor" — 256 / 64 — ported from `pool.zig:34-40`. The 256
/// is real and defined elsewhere; the 64 was asserted and never derived, by
/// either implementation, and four connections is what it produced.
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
fn pool_size_default() -> u32 {
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
/// are `None` (`pool/inner.rs`, the `(None, None)` arm), and its defaults set
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
fn min_connections_default(max_connections: u32) -> u32 {
    const WARM_FRACTION: u32 = 4;
    (max_connections / WARM_FRACTION).max(1)
}

/// A starved pool fails fast rather than stalling for seconds and reading as a
/// slow request.
const ACQUIRE_TIMEOUT_MS_DEFAULT: u64 = 2_000;

/// The connection handshake budget, which is a different thing from the wait
/// for a free connection and is not tunable per role.
const CONNECT_TIMEOUT_MS_DEFAULT: u64 = 10_000;

const POOL_SIZE_KNOB: &str = "DATABASE_POOL_SIZE";
/// The warm floor, overridable for a host that wants a different one.
const MIN_POOL_SIZE_KNOB: &str = "DATABASE_MIN_POOL_SIZE";
const ACQUIRE_TIMEOUT_KNOB: &str = "DATABASE_ACQUIRE_TIMEOUT_MS";

/// Which connection a piece of work belongs on.
///
/// The roles are separated at the database, not just here: the migrator needs a
/// direct session endpoint because advisory locks are session-scoped, and the
/// API role runs with privileges the migrator's does not need.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DbRole {
    /// Background work and anything without a more specific role.
    Default,
    /// Request-path queries.
    Api,
    /// Schema migrations. Must be a session (not transaction-pooled) endpoint.
    Migrator,
}

impl DbRole {
    /// Every role, for callers that build the whole set.
    pub const ALL: &'static [Self] = &[Self::Default, Self::Api, Self::Migrator];

    /// The environment variable carrying this role's connection URL.
    #[must_use]
    pub const fn url_knob(self) -> &'static str {
        match self {
            Self::Default => "DATABASE_URL",
            Self::Api => "DATABASE_URL_API",
            Self::Migrator => "DATABASE_URL_MIGRATOR",
        }
    }

    /// The lower-case tag this role logs and reports under.
    #[must_use]
    pub const fn tag(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Api => "api",
            Self::Migrator => "migrator",
        }
    }

    /// The suffix a role-scoped knob override carries (`…_API`).
    const fn knob_suffix(self) -> &'static str {
        match self {
            Self::Default => "_DEFAULT",
            Self::Api => "_API",
            Self::Migrator => "_MIGRATOR",
        }
    }
}

/// One role's resolved connection settings.
#[derive(Debug, Clone)]
pub struct PoolConfig {
    role: DbRole,
    connect: PgConnectOptions,
    max_connections: u32,
    min_connections: u32,
    acquire_timeout: Duration,
    connect_timeout: Duration,
}

impl PoolConfig {
    /// Resolves a role's URL and limits from `env`.
    ///
    /// # Errors
    /// Returns a config error when the role's URL knob is unset, blank, or not
    /// a Postgres connection URL.
    pub fn resolve<E: EnvSource + ?Sized>(env: &E, role: DbRole) -> Result<Self> {
        let knob = role.url_knob();
        let url = env
            .get(knob)
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| Error::new(ErrorKind::MissingDatabaseUrl { knob }))?;

        let max_connections =
            read_knob(env, POOL_SIZE_KNOB, role).map_or_else(pool_size_default, clamp_pool_size);

        Ok(Self {
            role,
            // main's TLS-aware options, this branch's sizing: the two changed
            // different fields of the same literal.
            connect: tls::connect_options(role, &url)?,
            max_connections,
            // Never above the ceiling: a floor larger than the pool is a pool
            // that can never satisfy its own maintenance.
            min_connections: read_knob(env, MIN_POOL_SIZE_KNOB, role)
                .and_then(|raw| u32::try_from(raw).ok())
                .unwrap_or_else(|| min_connections_default(max_connections))
                .min(max_connections),
            acquire_timeout: Duration::from_millis(
                read_knob(env, ACQUIRE_TIMEOUT_KNOB, role).unwrap_or(ACQUIRE_TIMEOUT_MS_DEFAULT),
            ),
            connect_timeout: Duration::from_millis(CONNECT_TIMEOUT_MS_DEFAULT),
        })
    }

    /// The role these settings belong to.
    #[must_use]
    pub const fn role(&self) -> DbRole {
        self.role
    }

    /// How many connections this pool may open.
    #[must_use]
    pub const fn max_connections(&self) -> u32 {
        self.max_connections
    }

    /// How many connections are held open before anything asks for one.
    #[must_use]
    pub const fn min_connections(&self) -> u32 {
        self.min_connections
    }

    /// How long an acquire waits for a free connection before giving up.
    #[must_use]
    pub const fn acquire_timeout(&self) -> Duration {
        self.acquire_timeout
    }

    /// How long one connection handshake may take.
    #[must_use]
    pub const fn connect_timeout(&self) -> Duration {
        self.connect_timeout
    }

    /// The TLS posture this role resolved to, spelled as a connection URL
    /// spells it.
    ///
    /// Published rather than kept to the crate because it is the value an
    /// operator needs when a connection is refused, and because a posture
    /// nothing outside this module can read is a promise with no witness — the
    /// boot line reports it and this is what the boot line reads.
    #[must_use]
    pub fn ssl_mode(&self) -> &'static str {
        tls::ssl_mode_tag(self.connect.get_ssl_mode())
    }

    /// Shortens the handshake budget, for a test that means to exhaust it.
    ///
    /// The production value is a constant because a handshake budget is not an
    /// operator's decision — it is how long a TCP connection to a Postgres that
    /// accepts and then says nothing may hold boot open. Proving that timeout
    /// fires means waiting it out, and ten seconds per test is a lane nobody
    /// runs. Behind `test-util` so no deployment can shorten it.
    #[cfg(feature = "test-util")]
    #[must_use]
    pub const fn with_connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = timeout;
        self
    }

    /// The parsed connection options, for the pool builder.
    pub(crate) fn connect_options(&self) -> PgConnectOptions {
        self.connect.clone()
    }
}

/// Reads a numeric knob, preferring the role-scoped override over the base
/// name. Blank or unparseable is treated as absent, exactly as `pool.zig`'s
/// `parseEnvU32` does — a typo falls back to the default rather than refusing
/// to boot, because refusing here would take a serving daemon down over a knob.
fn read_knob<E: EnvSource + ?Sized>(env: &E, base: &str, role: DbRole) -> Option<u64> {
    let scoped = format!("{base}{}", role.knob_suffix());
    parse_knob(env, &scoped).or_else(|| parse_knob(env, base))
}

fn parse_knob<E: EnvSource + ?Sized>(env: &E, name: &str) -> Option<u64> {
    env.get(name)?.trim().parse::<u64>().ok()
}

/// Clamps a configured pool size into a usable connection count.
///
/// Zero is the case that matters: a zero-connection pool accepts every acquire
/// and satisfies none, so it hangs rather than fails, and the wrong knob value
/// looks like a datastore outage.
fn clamp_pool_size(raw: u64) -> u32 {
    match u32::try_from(raw) {
        Ok(0) | Err(_) => pool_size_default(),
        Ok(size) => size,
    }
}

/// How a boolean knob reads.
///
/// Three answers, not two: `MIGRATE_ON_START=maybe` is a misconfiguration an
/// operator has to see, and folding it into `No` is how a deploy silently
/// stops migrating (`config/load.zig:52-57`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvBool {
    /// `true` in any case, or `1`.
    Yes,
    /// `false` in any case, or `0`.
    No,
    /// Anything else.
    Invalid,
}

/// Parses the boolean grammar every env knob in this product shares.
#[must_use]
pub fn parse_env_bool(raw: &str) -> EnvBool {
    let trimmed = raw.trim();
    if trimmed.eq_ignore_ascii_case("true") || trimmed == "1" {
        EnvBool::Yes
    } else if trimmed.eq_ignore_ascii_case("false") || trimmed == "0" {
        EnvBool::No
    } else {
        EnvBool::Invalid
    }
}

#[cfg(test)]
mod pool_sizing_tests {
    use super::{min_connections_default, pool_size_default};

    /// The ceiling is a real number, not four.
    ///
    /// Four was `256 / 64`, and the 64 was a "per-connection request-sharing
    /// factor" that neither this implementation nor the Zig it was ported from
    /// ever derived. What four produced was a pool that spends its life below
    /// its own ceiling, which is the exact state `Db::acquire` reads as an
    /// absent datastore — so an undersized pool answered 503 while Postgres was
    /// healthy. This asserts the floor that failure sat under.
    #[test]
    fn test_the_pool_ceiling_is_not_the_undersized_default_it_replaced() {
        const THE_OLD_DEFAULT: u32 = 256 / 64;

        let ceiling = pool_size_default();

        assert!(
            ceiling > THE_OLD_DEFAULT,
            "the ceiling must exceed the four connections that produced 503s \
             under an ordinary burst: {ceiling}"
        );
        assert!(
            ceiling <= 100,
            "and must stay inside a stock Postgres connection ceiling, which is \
             what `core_api` sizes against: {ceiling}"
        );
    }

    /// Some of the pool is warm before any request arrives.
    ///
    /// This is the clause that removes a connection handshake from a request's
    /// path. A floor of zero is what `connect_lazy_with` gives on its own, and
    /// it is what made a cold pool's first traffic queue behind establishment
    /// — measured at 147 ms typically and 337 ms under load — inside a budget
    /// meant only for waiting.
    #[test]
    fn test_some_of_the_pool_is_established_before_it_is_asked_for() {
        let ceiling = pool_size_default();
        let warm = min_connections_default(ceiling);

        assert!(warm >= 1, "a pool that starts empty opens in the hot path");
        assert!(
            warm <= ceiling,
            "a floor above the ceiling is a pool that cannot satisfy its own \
             maintenance: {warm} > {ceiling}"
        );
    }
}
