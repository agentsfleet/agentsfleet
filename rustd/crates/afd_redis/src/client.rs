//! One connection per role, multiplexed, with a deadline on every command.
//!
//! # Why there is no pool
//!
//! The Zig daemon keeps a pool of request/reply sockets behind a mutex, because
//! a blocking client can only have one command in flight per socket. An async
//! client does not have that problem: `ConnectionManager` writes concurrent
//! commands down one socket and routes each reply back to the caller that is
//! waiting for it. So the ~3.0k lines of pooling, RESP framing and reconnect
//! logic under `src/agentsfleetd/queue/` become one field here — and the
//! reconnect that pool hand-rolled is the manager's own behaviour.
//!
//! The one thing a multiplexed connection must not do is run a blocking
//! command, because it would stall every other caller sharing the socket. That
//! is why pub/sub gets its own connection in [`crate::hub`] and why the stream
//! reads in [`crate::streams`] never pass `BLOCK`.
//!
//! # Deadlines are here, not in the caller
//!
//! Invariant 4: every I/O deadline is a `tokio::time::timeout` at the call
//! site. [`Redis::command`] is that call site, so no caller can start an
//! unbounded Redis operation by forgetting to wrap one.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use redis::aio::ConnectionManager;
#[cfg(feature = "test-util")]
use redis::aio::ConnectionManagerConfig;
use redis::{Cmd, FromRedisValue, Value};

use crate::config::{RedisConfig, RedisRole};
use crate::error::{self, Error, ErrorKind, Result};

/// Correlates one connection boundary's started and terminal records.
static NEXT_CONNECT_ATTEMPT: AtomicU64 = AtomicU64::new(0);

/// The liveness probe, and the only command this module issues by name.
const CMD_PING: &str = "PING";

/// A connection to one role's Redis.
///
/// Cheap to clone: cloning shares the same multiplexed connection rather than
/// opening another, which is what keeps "one connection per process per role"
/// true no matter how many components hold one.
#[derive(Debug, Clone)]
pub struct Redis {
    role: RedisRole,
    manager: ConnectionManager,
    request_timeout: Duration,
}

impl Redis {
    /// Opens the connection for `config`'s role, proving Redis answers.
    ///
    /// # Errors
    /// Returns an unavailable error when Redis cannot be reached, and a config
    /// error when a certificate authority file was named but not readable.
    pub async fn connect(config: &RedisConfig) -> Result<Self> {
        let started = Instant::now();
        let attempt_id = NEXT_CONNECT_ATTEMPT.fetch_add(1, Ordering::Relaxed);
        let role = config.role().tag();
        let timeout_ms = config.connect_timeout().as_millis();
        let tls = config.is_tls();
        tracing::info!(
            attempt_id,
            role,
            timeout_ms,
            tls,
            event = "redis_connect_started"
        );

        let result =
            match tokio::time::timeout(config.connect_timeout(), Self::connect_inner(config)).await
            {
                Ok(result) => result,
                Err(_elapsed) => Err(error::connect_timed_out(role, timeout_ms)),
            };
        let duration_ms = started.elapsed().as_millis();
        match result {
            Ok(redis) => {
                let request_timeout_ms = config.request_timeout().as_millis();
                tracing::info!(
                    attempt_id,
                    role,
                    duration_ms,
                    request_timeout_ms,
                    tls,
                    event = "redis_connect_completed"
                );
                Ok(redis)
            }
            Err(failure) => {
                let error_code = failure.code().as_str();
                tracing::warn!(
                    attempt_id,
                    role,
                    duration_ms,
                    error_code,
                    reason = %failure,
                    event = "redis_connect_failed"
                );
                Err(failure)
            }
        }
    }

    async fn connect_inner(config: &RedisConfig) -> Result<Self> {
        let client = build_client(config)?;
        let manager = ConnectionManager::new(client).await.map_err(|source| {
            Error::new(ErrorKind::Unreachable {
                role: config.role().tag(),
                source: Box::new(source),
            })
        })?;

        let redis = Self {
            role: config.role(),
            manager,
            request_timeout: config.request_timeout(),
        };
        // A connection that has not answered is a connection that might not
        // exist: `ConnectionManager::new` establishes one, but the boot
        // preflight's claim is that Redis SERVES, and only a reply proves that.
        redis.ping().await?;
        Ok(redis)
    }

    /// A handle over a Redis that has NOT been proven to answer.
    ///
    /// The mirror of [`afd_db::Db::unreachable`], and behind `test-util` for
    /// the same reason: the ping in [`Redis::connect`] is the promise that a
    /// boot which returned has a Redis that SERVES, and a constructor skipping
    /// it would let a binary start against a queue that is not there.
    ///
    /// What it exists for is the other half of that promise — proving what the
    /// request path does when the queue is gone. `ConnectionManager` is built
    /// lazily against the configured address, so every command through it
    /// fails at the socket rather than at a fake.
    ///
    /// # Why a test needs its OWN unreachable handle
    ///
    /// The integration lane's Redis is SHARED by every test binary running in
    /// parallel, so the obvious injections — pausing the container, killing the
    /// server, dropping the port — fail unrelated suites at the same instant.
    /// A handle only one test holds is the only way to prove the drop path
    /// without taking the queue away from everybody else.
    ///
    /// # Errors
    /// Returns a config error when a certificate authority file was named but
    /// not readable, and an unreachable error when the manager cannot even be
    /// constructed — both happen before any socket.
    #[cfg(feature = "test-util")]
    pub fn unreachable(config: &RedisConfig) -> Result<Self> {
        let client = build_client(config)?;
        // `new_lazy_with_config` builds the manager WITHOUT opening a socket,
        // which is the whole point: `connect` above opens one and pings it, and
        // this seam exists to skip exactly that.
        let manager =
            ConnectionManager::new_lazy_with_config(client, ConnectionManagerConfig::new())
                .map_err(|source| {
                    Error::new(ErrorKind::Unreachable {
                        role: config.role().tag(),
                        source: Box::new(source),
                    })
                })?;
        Ok(Self {
            role: config.role(),
            manager,
            request_timeout: config.request_timeout(),
        })
    }

    /// The role this connection serves.
    #[must_use]
    pub const fn role(&self) -> RedisRole {
        self.role
    }

    /// How long any one command may take.
    #[must_use]
    pub const fn request_timeout(&self) -> Duration {
        self.request_timeout
    }

    /// Runs one command under this connection's deadline.
    ///
    /// `name` is what a failure reports; it is the command verb rather than the
    /// whole argument vector, because arguments carry payloads and payloads do
    /// not belong in error text.
    ///
    /// # Errors
    /// Returns a timeout error when the deadline passes, a group-missing error
    /// for `NOGROUP`, an unavailable error when the connection dropped, and a
    /// command error otherwise.
    pub async fn command<T: FromRedisValue>(
        &self,
        name: &'static str,
        context: &str,
        cmd: &Cmd,
    ) -> Result<T> {
        let mut manager = self.manager.clone();
        let value =
            tokio::time::timeout(self.request_timeout, cmd.query_async::<Value>(&mut manager))
                .await
                .map_err(|_elapsed| error::timed_out(name, self.request_timeout.as_millis()))?
                .map_err(|source| error::classify(name, context, source))?;

        // A parse failure is not a Redis failure: the server answered, and the
        // reply is a shape this client did not expect. Reporting it as a
        // command error would send an operator looking at Redis.
        T::from_redis_value(value).map_err(|_parse| error::unexpected_reply(name))
    }

    /// Runs a prepared script invocation, under the same deadline a command
    /// gets.
    ///
    /// Its own method rather than a `Cmd`, because a script invocation is not
    /// one: `redis` loads the body by digest and falls back to sending it when
    /// the server has never seen it, and that retry is the crate's to perform.
    /// What this adds is what [`Self::command`] adds — the timeout, the error
    /// classification, and the rule that a reply shape we did not expect is
    /// reported as such rather than as a Redis fault.
    ///
    /// # Errors
    /// As [`Self::command`].
    pub async fn script<T: FromRedisValue>(
        &self,
        name: &'static str,
        context: &str,
        invocation: &redis::ScriptInvocation<'_>,
    ) -> Result<T> {
        let mut manager = self.manager.clone();
        let value = tokio::time::timeout(
            self.request_timeout,
            invocation.invoke_async::<Value>(&mut manager),
        )
        .await
        .map_err(|_elapsed| error::timed_out(name, self.request_timeout.as_millis()))?
        .map_err(|source| error::classify(name, context, source))?;

        T::from_redis_value(value).map_err(|_parse| error::unexpected_reply(name))
    }

    /// `PING`, which is how boot asks whether Redis is actually serving.
    ///
    /// # Errors
    /// Returns an unavailable error when Redis does not answer.
    pub async fn ping(&self) -> Result<()> {
        let reply: String = self.command(CMD_PING, "", &redis::cmd(CMD_PING)).await?;
        if reply.eq_ignore_ascii_case("PONG") {
            Ok(())
        } else {
            Err(error::unexpected_reply(CMD_PING))
        }
    }
}

/// Builds the client, wiring a custom certificate authority when one is named.
///
/// The local compose Redis serves a self-signed certificate, so the CA arrives
/// by path rather than from a trust store. `REDIS_TLS_CA_CERT_FILE` is the same
/// knob `redis_config.zig` reads, and the same file the Zig lane extracts from
/// the container.
pub(crate) fn build_client(config: &RedisConfig) -> Result<redis::Client> {
    let Some(path) = config.ca_cert_file() else {
        return redis::Client::open(config.url()).map_err(|source| {
            Error::new(ErrorKind::Unreachable {
                role: config.role().tag(),
                source: Box::new(source),
            })
        });
    };

    let root_cert = std::fs::read(path).map_err(|source| {
        Error::new(ErrorKind::CaCertUnreadable {
            path: path.display().to_string(),
            source,
        })
    })?;

    redis::Client::build_with_tls(
        config.url(),
        redis::TlsCertificates {
            client_tls: None,
            root_cert: Some(root_cert),
        },
    )
    .map_err(|source| {
        Error::new(ErrorKind::Unreachable {
            role: config.role().tag(),
            source: Box::new(source),
        })
    })
}
