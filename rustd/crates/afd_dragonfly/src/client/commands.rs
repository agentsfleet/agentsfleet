//! Bounded command execution and node routing.

use super::{CMD_PING, Dragonfly};
use crate::error::{self, Result};
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
    pub(super) async fn bounded<T: FromRedisValue>(
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
