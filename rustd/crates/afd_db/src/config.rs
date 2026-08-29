//! Which database a role connects to, and with what limits.
//!
//! Every knob name here is the Zig daemon's, spelled identically
//! (`src/agentsfleetd/db/pool.zig`): a deployment moves between the two
//! binaries without touching its environment, or the port is not a port.
//!
//! # TLS is required unless the URL says otherwise
//!
//! `sqlx` defaults to `sslmode=prefer` — encrypt if the server offers it,
//! continue in the clear if it does not. Every role-separated connection here
//! goes to a hosted provider that mandates TLS, so the default is `require`
//! and a URL that wants otherwise has to say `?sslmode=disable`, which is what
//! the local compose Postgres does.

use std::str::FromStr as _;
use std::time::Duration;

use sqlx::postgres::{PgConnectOptions, PgSslMode};

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
/// `core_api` sizes against two facts instead of an assumption — the host's CPU
/// count and Postgres's hundred-connection ceiling — and states the second in
/// `components/database/src/config.rs`: "This make sure the `max_connections` in
/// close to 100 in postgres."
///
/// Its shape is ported here and its NUMBER is taken too: fifty, which is what
/// `core_api` runs in `local.toml`, `testapi.toml` and `devapi.toml`. Its own
/// code default is forty and every one of its environments overrides upward,
/// so forty is the number nothing actually runs on.
///
/// `apiprod.toml` goes to seventy-five. That is left to the deployment rather
/// than written here, because at seventy-five a stock hundred-connection
/// Postgres serves a single replica — so it travels with a Postgres
/// `max_connections` raise, which is a deploy decision. The knob is
/// `DATABASE_POOL_SIZE`, role-suffixable as `DATABASE_POOL_SIZE_API`.
fn pool_size_default() -> u32 {
    const SMALL_HOST_CORES: usize = 4;
    const PER_CORE_ON_A_SMALL_HOST: u32 = 10;
    const CEILING: u32 = 50;

    let cores =
        std::thread::available_parallelism().map_or(SMALL_HOST_CORES, std::num::NonZero::get);
    // `cores` is bounded by SMALL_HOST_CORES on this branch, so the conversion
    // cannot lose anything; the fallback keeps that true without an unwrap.
    let small = u32::try_from(cores.min(SMALL_HOST_CORES)).unwrap_or(1);
    if cores <= SMALL_HOST_CORES {
        small * PER_CORE_ON_A_SMALL_HOST
    } else {
        CEILING
    }
}

/// How much of the pool is established BEFORE a request needs it.
///
/// The knob that stops a connection being opened in a request's path. sqlx
/// maintains this floor in the background (`min_connections_maintenance`), so a
/// burst finds live connections rather than a handshake apiece. Without it the
/// pool starts at zero — `connect_lazy_with` — and the first traffic after boot
/// pays the establishment cost the acquire budget was never meant to cover.
///
/// A quarter of the ceiling rather than all of it: connections held open cost
/// Postgres slots whether or not traffic exists, and the point is to remove the
/// cold start, not to reserve the whole pool against a burst that may not come.
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

/// The two spellings a Postgres URL may carry, and the only two.
const POSTGRES_SCHEMES: [&str; 2] = ["postgres://", "postgresql://"];

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
            connect: connect_options(knob, &url)?,
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

/// Parses a connection URL, defaulting TLS to required.
///
/// The default is applied only when the URL is silent: `?sslmode=disable` is
/// how the local compose Postgres — which serves no TLS at all — is reachable,
/// and honouring it is why the local lane works without a certificate.
fn connect_options(knob: &'static str, url: &str) -> Result<PgConnectOptions> {
    // The scheme is checked here rather than left to sqlx, which accepts
    // `mysql://host/db` and reads it as host `host`, database `db`. A
    // deployment that pasted the wrong URL then connects somewhere real and
    // fails on the first query instead of at boot. `parseUrl` refuses anything
    // but these two prefixes (`pool.zig:81-87`) and so does this.
    if !POSTGRES_SCHEMES
        .iter()
        .any(|scheme| url.starts_with(scheme))
    {
        return Err(Error::new(ErrorKind::InvalidDatabaseUrlScheme { knob }));
    }

    let options = PgConnectOptions::from_str(url).map_err(|source| {
        Error::new(ErrorKind::InvalidDatabaseUrl {
            knob,
            source: Box::new(source),
        })
    })?;
    if url_declares_sslmode(url) {
        Ok(options)
    } else {
        Ok(options.ssl_mode(PgSslMode::Require))
    }
}

/// Whether the URL's query string names `sslmode` at all.
fn url_declares_sslmode(url: &str) -> bool {
    url.split_once('?').is_some_and(|(_, query)| {
        query
            .split('&')
            .any(|param| param.split('=').next() == Some("sslmode"))
    })
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
