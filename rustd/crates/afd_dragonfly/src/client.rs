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
//! [`Redis::command`] is that call site, so no caller can start an unbounded
//! datastore operation by forgetting to wrap one.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use redis::cluster_async::ClusterConnection;
use redis::cluster_routing::{RoutingInfo, SingleNodeRoutingInfo};
use redis::{Cmd, FromRedisValue, Value};

use crate::config::{RedisConfig, RedisRole};
use crate::error::{self, Result};
use crate::transport;

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
pub struct Redis {
    role: RedisRole,
    connection: ClusterConnection,
    request_timeout: Duration,
}

impl std::fmt::Debug for Redis {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Redis")
            .field("role", &self.role)
            .field("request_timeout", &self.request_timeout)
            .finish_non_exhaustive()
    }
}

impl Redis {
    /// Opens the connection for `config`'s role, proving the cluster answers.
    ///
    /// # Errors
    /// Returns an unavailable error when the cluster cannot be reached, and a
    /// config error when a certificate authority file was named but not
    /// readable.
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
        let connection = transport::connect(config, config.request_timeout()).await?;
        let redis = Self {
            role: config.role(),
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
    /// the same reason: the ping in [`Redis::connect`] is the promise that a
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
    pub fn unreachable(config: &RedisConfig) -> Result<Self> {
        Ok(Self {
            role: config.role(),
            connection: transport::pending(config, config.request_timeout())?,
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

    /// Runs one command under this connection's deadline, routed by its key.
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
        let mut connection = self.connection.clone();
        self.bounded(name, context, cmd.query_async::<Value>(&mut connection))
            .await
    }

    /// One `INFO <section>` reply per PRIMARY, in the order the topology names
    /// them.
    ///
    /// `INFO` has no key, so a cluster connection fans it out and answers a
    /// reply per node — which is not the single bulk string a caller decoding
    /// `String` expects, and is why asking for one returns an unexpected-reply
    /// error rather than a number. A server statistic on a cluster is a SUM
    /// across primaries anyway: memory a fleet's keys occupy is spread over
    /// whichever shard owns each key, and a figure from one node is a fraction
    /// reported as a total.
    ///
    /// Routed per node the way [`Self::scan_keys`] walks them, and
    /// concurrently, because the nodes are independent.
    ///
    /// # Errors
    /// As [`Self::command`], and when the topology cannot be read.
    pub async fn info_per_primary(&self, section: &str) -> Result<Vec<String>> {
        let primaries = crate::topology::primaries(self).await?;
        let asks = primaries.into_iter().map(|node| {
            let mut cmd = redis::cmd(CMD_INFO);
            cmd.arg(section);
            async move {
                let routing = RoutingInfo::SingleNode(SingleNodeRoutingInfo::ByAddress {
                    host: node.host,
                    port: node.port,
                });
                self.route::<String>(CMD_INFO, section, &cmd, routing).await
            }
        });
        futures_util::future::try_join_all(asks).await
    }

    /// Runs one command on the node at `routing`, for the verbs that have no
    /// key to route by — a `SCAN` is a walk of one node.
    ///
    /// # Errors
    /// As [`Self::command`].
    pub(crate) async fn route<T: FromRedisValue>(
        &self,
        name: &'static str,
        context: &str,
        cmd: &Cmd,
        routing: RoutingInfo,
    ) -> Result<T> {
        let mut connection = self.connection.clone();
        let cmd = cmd.clone();
        self.bounded(name, context, async move {
            connection.route_command(cmd, routing).await
        })
        .await
    }

    /// One reply from ONE node, for a command that describes the DEPLOYMENT
    /// rather than a key.
    ///
    /// `CLUSTER INFO` and `COMMAND INFO` answer the same on every node — they
    /// say what the server IS and what it can do, not what it holds — so
    /// asking one node is asking all of them. Routed explicitly rather than
    /// left to the driver, which fans a keyless command out and answers a
    /// reply per node: not the single value a caller decoding one expects,
    /// which is the same trap [`Self::info_per_primary`] exists for.
    ///
    /// # Errors
    /// As [`Self::command`].
    pub(crate) async fn ask_one_node<T: FromRedisValue>(
        &self,
        name: &'static str,
        context: &str,
        cmd: &Cmd,
    ) -> Result<T> {
        self.route(
            name,
            context,
            cmd,
            RoutingInfo::SingleNode(SingleNodeRoutingInfo::Random),
        )
        .await
    }

    /// Runs a prepared script invocation, under the same deadline a command
    /// gets.
    ///
    /// Its own method rather than a `Cmd`, because a script invocation is not
    /// one: the driver loads the body by digest and falls back to sending it
    /// when the server has never seen it, and that retry is the driver's to
    /// perform.
    ///
    /// # Errors
    /// As [`Self::command`].
    pub async fn script<T: FromRedisValue>(
        &self,
        name: &'static str,
        context: &str,
        invocation: &redis::ScriptInvocation<'_>,
    ) -> Result<T> {
        let mut connection = self.connection.clone();
        self.bounded(
            name,
            context,
            invocation.invoke_async::<Value>(&mut connection),
        )
        .await
    }

    /// The deadline, the error classification, and the rule that a reply
    /// shape we did not expect is reported as such rather than as a datastore
    /// fault — applied once, to every path.
    ///
    /// RESP3 carries a server error as a VALUE (`Value::ServerError`) rather
    /// than a transport-level `Err`, so `extract_error` lifts it before
    /// classification; otherwise `NOGROUP` would decode as an unexpected reply
    /// instead of the recoverable class it is.
    async fn bounded<T: FromRedisValue>(
        &self,
        name: &'static str,
        context: &str,
        query: impl Future<Output = redis::RedisResult<Value>>,
    ) -> Result<T> {
        let waited_ms = self.request_timeout.as_millis();
        let value = tokio::time::timeout(self.request_timeout, query)
            .await
            .map_err(|_elapsed| error::timed_out(name, waited_ms))?
            .and_then(Value::extract_error)
            // Two deadlines race on every command and either may win: this
            // one, and the driver's own `response_timeout`, which
            // `transport::builder` sets from the same budget. The driver's
            // surfaces as a redis error, and `classify` reads it as a dropped
            // connection -- true, but `Unreachable` renders only the role, so
            // the command that actually hung is dropped from the sentence an
            // operator reads. The command and the budget are this call site's
            // knowledge; naming them here is what rule 3 keeps a `map_err`
            // for.
            .map_err(|source| {
                if source.is_timeout() {
                    error::timed_out(name, waited_ms)
                } else {
                    error::classify(name, context, source)
                }
            })?;
        // A parse failure is not a datastore failure: the server answered, and
        // the reply is a shape this client did not expect. Reporting it as a
        // command error would send an operator looking at the datastore.
        T::from_redis_value(value).map_err(|_parse| error::unexpected_reply(name))
    }

    /// `PING`, which is how boot asks whether the cluster is actually serving.
    ///
    /// # Errors
    /// Returns an unavailable error when the cluster does not answer.
    pub async fn ping(&self) -> Result<()> {
        let reply: String = self.command(CMD_PING, "", &redis::cmd(CMD_PING)).await?;
        if reply.eq_ignore_ascii_case("PONG") {
            Ok(())
        } else {
            Err(error::unexpected_reply(CMD_PING))
        }
    }
}
