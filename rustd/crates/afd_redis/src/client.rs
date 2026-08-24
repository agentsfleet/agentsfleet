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

use std::time::Duration;

use redis::aio::ConnectionManager;
use redis::{Cmd, FromRedisValue, Value};

use crate::config::{RedisConfig, RedisRole};
use crate::error::{self, Error, ErrorKind};

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
    pub async fn connect(config: &RedisConfig) -> Result<Self, Error> {
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

        tracing::info!(
            role = config.role().tag(),
            request_timeout_ms = config.request_timeout().as_millis(),
            tls = config.is_tls(),
            "redis_connected"
        );
        Ok(redis)
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
    ) -> Result<T, Error> {
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

    /// `PING`, which is how boot asks whether Redis is actually serving.
    ///
    /// # Errors
    /// Returns an unavailable error when Redis does not answer.
    pub async fn ping(&self) -> Result<(), Error> {
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
pub(crate) fn build_client(config: &RedisConfig) -> Result<redis::Client, Error> {
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
