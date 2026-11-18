//! Which database a role connects to, and with what limits.
//!
//! Every knob name here is one a deployment already exports (`DATABASE_URL`
//! and its role-scoped variants, `DATABASE_POOL_SIZE` and the other limits
//! below), so configuring a pool is a matter of the environment a deployment
//! already has.
//!
//! The TLS posture a URL resolves to lives in [`tls`], with the reasoning that
//! explains it.

use std::time::Duration;

use sqlx::postgres::PgConnectOptions;

mod budget;
mod sizing;
mod tls;

pub use budget::{PoolOverrides, PoolSettings};

use self::sizing::{
    ACQUIRE_TIMEOUT_DEFAULT, CONNECT_TIMEOUT_DEFAULT, min_connections_default, pool_size_default,
};
use crate::error::{Error, ErrorKind, Result};
use af_core::env::{EnvSource, optional};

const POOL_SIZE_KNOB: &str = "DATABASE_POOL_SIZE";
/// The warm floor, overridable for a host that wants a different one.
const MIN_POOL_SIZE_KNOB: &str = "DATABASE_MIN_POOL_SIZE";
pub(crate) const ACQUIRE_TIMEOUT_KNOB: &str = "DATABASE_ACQUIRE_TIMEOUT_MS";

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
    idle_timeout: Duration,
    max_lifetime: Duration,
    ping_after_idle: Duration,
}

impl PoolConfig {
    /// Resolves a role's URL and limits from `env`.
    ///
    /// # Errors
    /// Refuses an invalid pool budget or a missing, blank or unusable role URL.
    pub fn resolve<E: EnvSource + ?Sized>(env: &E, role: DbRole) -> Result<Self> {
        let settings = PoolSettings::resolve(env, role, PoolOverrides::default())?;
        Self::configured(env, role, &settings)
    }

    /// Adds the role's secret URL to an already checked public connection budget.
    ///
    /// # Errors
    /// Refuses a missing or unusable URL before any connection opens.
    pub fn configured(
        env: &(impl EnvSource + ?Sized),
        role: DbRole,
        settings: &garde::Valid<PoolSettings>,
    ) -> Result<Self> {
        let knob = role.url_knob();
        let url = optional(env, knob)
            .ok_or_else(|| Error::new(ErrorKind::MissingDatabaseUrl { knob }))?;
        Ok(Self {
            role,
            connect: tls::connect_options(role, &url)?,
            max_connections: settings.max_connections,
            min_connections: settings.min_connections,
            acquire_timeout: Duration::from_millis(settings.acquire_timeout_ms),
            connect_timeout: Duration::from_millis(settings.connect_timeout_ms),
            idle_timeout: Duration::from_secs(settings.idle_timeout_seconds),
            max_lifetime: Duration::from_secs(settings.max_lifetime_seconds),
            ping_after_idle: Duration::from_millis(settings.ping_after_idle_ms),
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

    /// How long an acquire may take, the handshake included when the pool has
    /// to open one. See [`ACQUIRE_TIMEOUT_DEFAULT`]: sqlx spends this one
    /// deadline on both halves.
    #[must_use]
    pub const fn acquire_timeout(&self) -> Duration {
        self.acquire_timeout
    }

    /// How long one connection handshake may take.
    #[must_use]
    pub const fn connect_timeout(&self) -> Duration {
        self.connect_timeout
    }

    /// How long a pooled connection can remain unused.
    #[must_use]
    pub const fn idle_timeout(&self) -> Duration {
        self.idle_timeout
    }

    /// How long a pooled connection can remain alive.
    #[must_use]
    pub const fn max_lifetime(&self) -> Duration {
        self.max_lifetime
    }

    /// Minimum idle interval before a checkout tests the connection.
    #[must_use]
    pub const fn ping_after_idle(&self) -> Duration {
        self.ping_after_idle
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

/// How a boolean knob reads.
///
/// Three answers, not two: `MIGRATE_ON_START=maybe` is a misconfiguration an
/// operator has to see, and folding it into `No` is how a deploy silently
/// stops migrating.
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
mod pool_sizing_tests;
