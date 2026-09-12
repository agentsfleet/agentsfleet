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
//! # The deadline covers the park, and the caller declares the park
//!
//! Every command through [`Redis::command`] carries `request_timeout`, because
//! a request-path command that hangs is an outage. A blocking read is
//! different: parking IS the behaviour, so its deadline has to be LONGER than
//! the longest park a caller will ask for, or the driver gives up on a read
//! the server is still honouring. That is not hypothetical — the driver's own
//! default reply deadline is half a second, and a connection opened without
//! naming a longer one fails every `BLOCK 5000` at 500 ms while the server
//! keeps the socket parked for the remaining four and a half. Every command
//! queued behind it then times out too, and the reader never sees an entry.
//!
//! So [`Dedicated::connect`] takes the longest park the owner will request,
//! and the reply deadline is that park plus the role's `request_timeout`: the
//! server's bound, then the ordinary allowance for the answer to travel. Still
//! a bound — a peer that vanishes without closing the socket is noticed, and
//! `BLOCK 0` (wait forever) is refused by construction because no park is
//! declared for it.
//!
//! The DIAL is not governed by this allowance, which is worth stating because
//! it looks as though it should be. The driver wraps the whole setup — socket,
//! handshake and all — in its own `connection_timeout`, and the retry policy
//! still bounds each attempt at `CONNECT_ATTEMPT_TIMEOUT`. So `client.rs`'s
//! ladder arithmetic holds here too: the driver's own error is still the first
//! to fire on a peer that accepts a socket and then says nothing, and it keeps
//! its source chain. Raising the REPLY deadline buys the park without spending
//! the dial's diagnostics.
//!
//! # A dropped socket heals, as the shared handle's does
//!
//! The socket is held through the driver's [`ConnectionManager`], the same way
//! [`Redis`] holds its own: a command that meets a dead socket fails, the
//! manager redials in the background, and the next command goes down the new
//! socket. The ownership rule above is unchanged — the manager is `Clone`, and
//! this type does not hand it out.
use std::time::Duration;

use redis::aio::ConnectionManager;
use redis::{Cmd, FromRedisValue, Value};

use crate::client::{build_client, connect_retry_policy};
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
    manager: ConnectionManager,
}

impl Dedicated {
    /// Opens a connection for `config`'s role that this caller alone holds.
    ///
    /// `longest_park` is the longest `BLOCK` the owner will ever pass: the
    /// reply deadline on every command is that plus the role's
    /// `request_timeout`, so a read the server is still honouring is never
    /// given up on — see the module note.
    ///
    /// Unlike [`crate::Redis::connect`] there is no ping: the caller is a
    /// background consumer rather than boot, and a consumer that cannot reach
    /// Redis retries rather than failing a process that is otherwise healthy.
    /// Boot's promise that Redis SERVES is made once, by the shared handle.
    ///
    /// # Errors
    /// Returns an unavailable error when Redis cannot be reached within the
    /// role's `connect_timeout`, and a config error when a certificate
    /// authority file was named but not readable.
    pub async fn connect(config: &RedisConfig, longest_park: Duration) -> Result<Self> {
        let role = config.role().tag();
        let client = build_client(config)?;
        let policy = connect_retry_policy()
            .set_response_timeout(Some(longest_park + config.request_timeout()));
        let dial = ConnectionManager::new_with_config(client, policy);
        let manager = match tokio::time::timeout(config.connect_timeout(), dial).await {
            Ok(dialed) => dialed.map_err(|source| {
                Error::new(ErrorKind::Unreachable {
                    role,
                    source: Box::new(source),
                })
            })?,
            Err(_elapsed) => {
                return Err(error::connect_timed_out(
                    role,
                    config.connect_timeout().as_millis(),
                ));
            }
        };

        // Hoisted: see the `tracing` note in the workspace Cargo.toml.
        let park_ms = longest_park.as_millis();
        tracing::debug!(role, park_ms, event = "redis_dedicated_connected");
        Ok(Self {
            role: config.role(),
            manager,
        })
    }

    /// The role this connection serves.
    #[must_use]
    pub const fn role(&self) -> RedisRole {
        self.role
    }

    /// Runs one command under the deadline the connection was opened with.
    ///
    /// `&mut self` rather than `&self`, which is the invariant stated as a
    /// signature: a second concurrent command on a socket that may be parked
    /// is the failure this type prevents, and here it does not compile.
    ///
    /// # Errors
    /// Returns a group-missing error for `NOGROUP`, an unavailable error when
    /// the connection dropped or the deadline passed, a command error
    /// otherwise, and an unexpected-reply error when Redis answers a shape `T`
    /// cannot read.
    pub async fn command<T: FromRedisValue>(
        &mut self,
        name: &'static str,
        context: &str,
        cmd: &Cmd,
    ) -> Result<T> {
        let value = cmd
            .query_async::<Value>(&mut self.manager)
            .await
            .map_err(|source| error::classify(name, context, source))?;

        // A parse failure is not a Redis failure — see [`crate::Redis::command`].
        T::from_redis_value(value).map_err(|_parse| error::unexpected_reply(name))
    }
}
