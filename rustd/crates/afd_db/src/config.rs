//! Which database a role connects to, and with what limits.
//!
//! Every knob name here is the Zig daemon's, spelled identically
//! (`src/agentsfleetd/db/pool.zig`): a deployment moves between the two
//! binaries without touching its environment, or the port is not a port.
//!
//! The TLS posture a URL resolves to lives in [`tls`], with the history that
//! explains it.

use std::time::Duration;

use sqlx::postgres::PgConnectOptions;

mod tls;

use crate::error::{Error, ErrorKind, Result};
use afd_core::env::EnvSource;

/// Pool size default: the API in-flight ceiling divided by the number of
/// requests that share one connection. Many concurrent requests need far fewer
/// connections than they are requests, so the pool does not scale 1:1 with
/// admission (`pool.zig:34-40`).
const API_MAX_IN_FLIGHT_REQUESTS_DEFAULT: u32 = 256;
const POOL_SIZE_INFLIGHT_DIVISOR: u32 = 64;
const POOL_SIZE_DEFAULT: u32 = API_MAX_IN_FLIGHT_REQUESTS_DEFAULT / POOL_SIZE_INFLIGHT_DIVISOR;

/// A starved pool fails fast rather than stalling for seconds and reading as a
/// slow request.
const ACQUIRE_TIMEOUT_MS_DEFAULT: u64 = 2_000;

/// The connection handshake budget, which is a different thing from the wait
/// for a free connection and is not tunable per role.
const CONNECT_TIMEOUT_MS_DEFAULT: u64 = 10_000;

const POOL_SIZE_KNOB: &str = "DATABASE_POOL_SIZE";
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

        Ok(Self {
            role,
            connect: tls::connect_options(role, &url)?,
            max_connections: read_knob(env, POOL_SIZE_KNOB, role)
                .map_or(POOL_SIZE_DEFAULT, clamp_pool_size),
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
        Ok(0) | Err(_) => POOL_SIZE_DEFAULT,
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
