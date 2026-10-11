//! One cluster connection per role, multiplexed, with a deadline on every
//! command.
//!
//! # Why there is no pool
//!
//! The driver keeps one socket per cluster node and writes concurrent commands
//! down each, routing every reply back to the caller that is waiting for it.
//! A pool would add sockets without adding throughput. The one thing a shared
//! connection must not carry is a blocking command, because the driver applies
//! a single reply deadline per connection and a parked read holds the owning
//! node's only socket: that is why pub/sub and blocking reads each hold their
//! own connection ([`crate::hub`], [`crate::Dedicated`]).
//!
//! # Deadlines are here, not in the caller
//!
//! Every I/O deadline is a `tokio::time::timeout` at the call site.
//! [`Dragonfly::command`] is that call site, so no caller can start an unbounded
//! datastore operation by forgetting to wrap one.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use redis::cluster_async::ClusterConnection;

use crate::config::{DragonflyConfig, DragonflyRole};
use crate::error::{self, Result};
use crate::transport;

mod commands;

/// Correlates one connection boundary's started and terminal records.
static NEXT_CONNECT_ATTEMPT: AtomicU64 = AtomicU64::new(0);

/// The liveness probe, and the only command this module issues by name.
/// The server's own statistics. No key, so it routes per node.
const CMD_INFO: &str = "INFO";

const CMD_PING: &str = "PING";

/// A connection to one role's cluster.
///
/// Cheap to clone: cloning shares the same connection rather than opening
/// another, which is what keeps "one connection per process per role" true no
/// matter how many components hold one.
#[derive(Clone)]
pub struct Dragonfly {
    role: DragonflyRole,
    connection: ClusterConnection,
    request_timeout: Duration,
    pub(crate) namespace: crate::namespace::Namespace,
}

impl std::fmt::Debug for Dragonfly {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Dragonfly")
            .field("role", &self.role)
            .field("request_timeout", &self.request_timeout)
            .finish_non_exhaustive()
    }
}

impl Dragonfly {
    /// Opens the connection for `config`'s role, proving the cluster answers.
    ///
    /// # Errors
    /// Returns an unavailable error when the cluster cannot be reached, and a
    /// config error when a certificate authority file was named but not
    /// readable.
    pub async fn connect(config: &DragonflyConfig) -> Result<Self> {
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
            event = "dragonfly_connect_started"
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
                    event = "dragonfly_connect_completed"
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
                    event = "dragonfly_connect_failed"
                );
                Err(failure)
            }
        }
    }

    async fn connect_inner(config: &DragonflyConfig) -> Result<Self> {
        let connection = transport::connect(config, config.request_timeout()).await?;
        let redis = Self {
            role: config.role(),
            namespace: config.namespace.clone(),
            connection,
            request_timeout: config.request_timeout(),
        };
        // A connection that has not answered is a connection that might not
        // exist: the driver has read the slot map, but the boot preflight's
        // claim is that the cluster SERVES, and only a reply proves that.
        redis.ping().await?;
        Ok(redis)
    }

    /// A handle over a cluster that has NOT been proven to answer.
    ///
    /// The mirror of [`afd_db::Db::unreachable`], and behind `test-util` for
    /// the same reason: the ping in [`Dragonfly::connect`] is the promise that a
    /// boot which returned has a datastore that SERVES, and a constructor
    /// skipping it would let a binary start against a queue that is not there.
    /// What it exists for is proving what the request path does when the
    /// queue is gone: the driver dials in the background, so every command
    /// through it fails at the socket rather than at a fake.
    ///
    /// # Errors
    /// Returns a config error when a certificate authority file was named but
    /// not readable, and an unreachable error when the seed is not a URL —
    /// both before any socket.
    #[cfg(feature = "test-util")]
    pub fn unreachable(config: &DragonflyConfig) -> Result<Self> {
        Ok(Self {
            role: config.role(),
            namespace: config.namespace.clone(),
            connection: transport::pending(config, config.request_timeout())?,
            request_timeout: config.request_timeout(),
        })
    }

    /// The role this connection serves.
    #[must_use]
    pub const fn role(&self) -> DragonflyRole {
        self.role
    }

    pub(crate) fn key(&self, logical: &str) -> String {
        self.namespace.key(logical)
    }

    /// How long any one command may take.
    #[must_use]
    pub const fn request_timeout(&self) -> Duration {
        self.request_timeout
    }
}
