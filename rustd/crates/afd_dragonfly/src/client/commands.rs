//! Bounded command execution and node routing.

use super::{CMD_INFO, CMD_PING, Dragonfly};
use crate::error::{self, Result};
use redis::cluster_routing::{RoutingInfo, SingleNodeRoutingInfo};
use redis::{Cmd, FromRedisValue, Pipeline, Value};

impl Dragonfly {
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

    /// Runs an ordered, single-slot command batch under one deadline.
    ///
    /// # Errors
    /// As [`Self::command`]. Callers must put every command on the same
    /// cluster slot; this API does not scatter a batch across shards.
    pub async fn pipeline<T: FromRedisValue>(
        &self,
        name: &'static str,
        context: &str,
        pipeline: &Pipeline,
    ) -> Result<T> {
        let mut connection = self.connection.clone();
        self.bounded(
            name,
            context,
            pipeline.query_async::<Value>(&mut connection),
        )
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
