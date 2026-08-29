//! A connection one component owns alone, so it may block on it.
//!
//! # What "dedicated" buys, and why [`Redis`] cannot be it
//!
//! [`Redis`] is shared by everything in the process: cloning it shares one
//! socket, which is the property that makes a pool unnecessary. The cost is
//! that a blocking command on it is not a slow command, it is a STOPPED
//! process — Redis executes commands on a connection in order, so an
//! `XREADGROUP … BLOCK 5000` parked at the head of the queue holds every other
//! caller's command behind it for five seconds. `client.rs` says as much, and
//! `streams/consume.rs` never passes `BLOCK` because of it.
//!
//! A consumer that wants to park has to bring its own socket. That is the
//! whole of this type: a connection with no other holder, opened by the one
//! component that will block on it. [`crate::hub`] is the precedent — pub/sub
//! takes a connection over, so the hub owns one and multiplexes locally.
//!
//! # Not cloneable, deliberately
//!
//! `Redis` is `Clone` because sharing it is correct. This is not, because
//! sharing it would reintroduce exactly the problem it exists to avoid: a
//! second holder issuing a command behind a parked read waits for the park.
//! One owner is the invariant, and taking `&mut self` on every call is how it
//! is stated — a caller that wanted two concurrent commands could not write
//! them.
//!
//! # No deadline here, and that is the point
//!
//! Every command through [`Redis::command`] carries `request_timeout`, because
//! a request-path command that hangs is an outage. A blocking read is the
//! opposite: parking IS the behaviour, and a timeout around it would fire on
//! every idle interval and turn a healthy quiet stream into a log full of
//! failures. The bound belongs in the `BLOCK` argument, which the server
//! honours and answers with an empty reply — see
//! [`crate::outbound::OutboundReader`].

use redis::aio::MultiplexedConnection;
use redis::{Cmd, FromRedisValue, Value};

use crate::client::build_client;
use crate::config::{RedisConfig, RedisRole};
use crate::error::{self, Error, ErrorKind, Result};

/// A Redis connection with exactly one owner.
///
/// See the module note: this exists so a component may issue a command that
/// parks — and it is the type system, not a comment, that keeps a second
/// caller off the socket.
#[derive(Debug)]
pub struct Dedicated {
    role: RedisRole,
    connection: MultiplexedConnection,
}

impl Dedicated {
    /// Opens a connection for `config`'s role that this caller alone holds.
    ///
    /// Unlike [`crate::Redis::connect`] there is no ping: the caller is a
    /// background consumer rather than boot, and a consumer that cannot reach
    /// Redis retries rather than failing a process that is otherwise healthy.
    /// Boot's promise that Redis SERVES is made once, by the shared handle.
    ///
    /// # Errors
    /// Returns an unavailable error when Redis cannot be reached, and a config
    /// error when a certificate authority file was named but not readable.
    pub async fn connect(config: &RedisConfig) -> Result<Self> {
        let client = build_client(config)?;
        let connection = client
            .get_multiplexed_async_connection()
            .await
            .map_err(|source| {
                Error::new(ErrorKind::Unreachable {
                    role: config.role().tag(),
                    source: Box::new(source),
                })
            })?;

        // Hoisted: see the `tracing` note in the workspace Cargo.toml.
        let role = config.role().tag();
        tracing::debug!(role, event = "redis_dedicated_connected");
        Ok(Self {
            role: config.role(),
            connection,
        })
    }

    /// The role this connection serves.
    #[must_use]
    pub const fn role(&self) -> RedisRole {
        self.role
    }

    /// Runs one command, with no deadline of this crate's.
    ///
    /// `&mut self` rather than `&self`, which is the invariant stated as a
    /// signature: a second concurrent command on a socket that may be parked
    /// is the failure this type prevents, and here it does not compile.
    ///
    /// # Errors
    /// Returns a group-missing error for `NOGROUP`, an unavailable error when
    /// the connection dropped, a command error otherwise, and an
    /// unexpected-reply error when Redis answers a shape `T` cannot read.
    pub async fn command<T: FromRedisValue>(
        &mut self,
        name: &'static str,
        context: &str,
        cmd: &Cmd,
    ) -> Result<T> {
        let value = cmd
            .query_async::<Value>(&mut self.connection)
            .await
            .map_err(|source| error::classify(name, context, source))?;

        // A parse failure is not a Redis failure — see [`crate::Redis::command`].
        T::from_redis_value(value).map_err(|_parse| error::unexpected_reply(name))
    }
}
