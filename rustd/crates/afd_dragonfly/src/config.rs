//! Which Dragonfly a role talks to, and how long it may take.
//!
//! One URL knob, not one per role. The two roles dial the SAME cluster and
//! differ only in the tag they log under, so a second name bought a second way
//! to misconfigure the same endpoint and nothing else. The knobs were the Zig
//! daemon's, spelled identically so a deployment could move between the two
//! binaries without touching its environment; that daemon is retired, which is
//! what freed these names to say what they now connect to.

use std::path::PathBuf;
use std::time::Duration;

use afd_core::env::EnvSource;

use crate::error::{Error, ErrorKind, Result};

const REQUEST_TIMEOUT_KNOB: &str = "DRAGONFLY_REQUEST_TIMEOUT_MS";
const REQUEST_TIMEOUT_MS_DEFAULT: u64 = 5_000;
const CONNECT_TIMEOUT_KNOB: &str = "DRAGONFLY_CONNECT_TIMEOUT_MS";
const CONNECT_TIMEOUT_MS_DEFAULT: u64 = 5_000;

/// The one URL every role dials.
pub const URL_KNOB: &str = "DRAGONFLY_URL";

/// Where a self-signed certificate authority is read from, for the local
/// compose Dragonfly. Unset means the system trust store.
pub const CA_CERT_FILE_KNOB: &str = "DRAGONFLY_TLS_CA_CERT_FILE";

/// Which connection a piece of work belongs on.
///
/// Two roles, not three: Dragonfly has no migrator. `redis_types.zig` carries the
/// same pair for the same reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RedisRole {
    /// Background work and anything without a more specific role.
    Default,
    /// Request-path commands.
    Api,
}

impl RedisRole {
    /// Every role, for callers that build the whole set.
    pub const ALL: &'static [Self] = &[Self::Default, Self::Api];

    /// The environment variable carrying every role's URL.
    ///
    /// One name for both, because both dial the same cluster. A role still
    /// exists to say which connection a log line or a fault came from; it no
    /// longer decides where that connection points.
    #[must_use]
    pub const fn url_knob(self) -> &'static str {
        URL_KNOB
    }

    /// The lower-case tag this role logs and reports under.
    #[must_use]
    pub const fn tag(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Api => "api",
        }
    }
}

/// One role's resolved connection settings.
#[derive(Debug, Clone)]
pub struct RedisConfig {
    role: RedisRole,
    url: String,
    ca_cert_file: Option<PathBuf>,
    connect_timeout: Duration,
    request_timeout: Duration,
}

impl RedisConfig {
    /// Resolves a role's URL, certificate path and deadline from `env`.
    ///
    /// # Errors
    /// Returns a config error when the role's URL knob is unset, blank, or not
    /// a Dragonfly URL.
    pub fn resolve<E: EnvSource + ?Sized>(env: &E, role: RedisRole) -> Result<Self> {
        let knob = role.url_knob();
        let url = env
            .get(knob)
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| Error::new(ErrorKind::MissingRedisUrl { knob }))?;

        // Through the client's own parser rather than a scheme prefix. A
        // prefix test is not a parse: `redis://[::1` and `redis://h:notaport`
        // both start with the right seven characters and neither is a URL, so
        // the hand-written check passed them to `Client::open` and the operator
        // met a typo as an UNREACHABLE at connect time — a message pointing at
        // the network for a fault in the environment. `parse_redis_url` is what
        // the transport builder will run on this string anyway, so validating with
        // anything else is a second, disagreeing opinion about the same value.
        //
        // It also accepts what the client accepts — `valkey://`, `unix://`,
        // `redis+unix://` — which the pair of literals did not. That widening is
        // the point rather than a side effect: this check exists to catch a
        // typo, not to be a stricter policy than the connection it guards.
        if redis::parse_redis_url(&url).is_none() {
            return Err(Error::new(ErrorKind::InvalidRedisUrl { knob }));
        }

        Ok(Self {
            role,
            url,
            ca_cert_file: env
                .get(CA_CERT_FILE_KNOB)
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty())
                .map(PathBuf::from),
            connect_timeout: timeout_from_env(
                env,
                CONNECT_TIMEOUT_KNOB,
                CONNECT_TIMEOUT_MS_DEFAULT,
            ),
            request_timeout: timeout_from_env(
                env,
                REQUEST_TIMEOUT_KNOB,
                REQUEST_TIMEOUT_MS_DEFAULT,
            ),
        })
    }

    /// Builds a configuration directly from a URL, for tests and for the
    /// subscription hub reusing an already-resolved connection string.
    #[must_use]
    pub fn from_url(role: RedisRole, url: String) -> Self {
        Self {
            role,
            url,
            ca_cert_file: None,
            connect_timeout: Duration::from_millis(CONNECT_TIMEOUT_MS_DEFAULT),
            request_timeout: Duration::from_millis(REQUEST_TIMEOUT_MS_DEFAULT),
        }
    }

    /// Points this configuration at a certificate authority file.
    #[must_use]
    pub fn with_ca_cert_file(mut self, path: Option<PathBuf>) -> Self {
        self.ca_cert_file = path;
        self
    }

    /// Shortens the whole connection-and-probe budget.
    #[must_use]
    pub const fn with_connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = timeout;
        self
    }

    /// Shortens the per-command deadline, for tests that must not wait out
    /// five seconds to prove a timeout is a timeout.
    #[must_use]
    pub const fn with_request_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = timeout;
        self
    }

    /// The role these settings belong to.
    #[must_use]
    pub const fn role(&self) -> RedisRole {
        self.role
    }

    /// The connection URL, as resolved.
    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Whether this connection is TLS, which is what `rediss://` means.
    #[must_use]
    pub fn is_tls(&self) -> bool {
        self.url.starts_with("rediss://")
    }

    /// The certificate authority to trust, when it is not the system's.
    #[must_use]
    pub fn ca_cert_file(&self) -> Option<&std::path::Path> {
        self.ca_cert_file.as_deref()
    }

    /// How long connection establishment and its liveness probe may take.
    #[must_use]
    pub const fn connect_timeout(&self) -> Duration {
        self.connect_timeout
    }

    /// How long any one command may take.
    #[must_use]
    pub const fn request_timeout(&self) -> Duration {
        self.request_timeout
    }
}

fn timeout_from_env<E: EnvSource + ?Sized>(env: &E, knob: &str, fallback_ms: u64) -> Duration {
    Duration::from_millis(
        env.get(knob)
            .and_then(|raw| raw.trim().parse::<u64>().ok())
            .filter(|millis| *millis > 0)
            .unwrap_or(fallback_ms),
    )
}
