//! A connection one component owns alone, so it may block on it.
//!
//! # What "dedicated" buys, and why [`Redis`] cannot be it
//!
//! [`Redis`] is shared by everything in the process: cloning it shares one
//! cluster connection, which holds exactly one socket per node. The driver
//! executes commands on a socket in order and applies one reply deadline to
//! every command on a connection, so an `XREADGROUP … BLOCK 5000` parked on
//! the shared handle holds the owning node's only socket for five seconds and
//! every other caller's command behind it — and raising the shared deadline
//! to cover the park would make every request-path hang wait that long too.
//! `streams/consume.rs` never passes `BLOCK` on the shared handle for exactly
//! that reason.
//!
//! A consumer that wants to park has to bring its own connection. That is the
//! whole of this type: a cluster connection with no other holder, opened by
//! the one component that will block on it, with a reply deadline sized to
//! its park. [`crate::hub`] is the precedent — pub/sub owns one too.
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
//! A blocking read is different from a request-path command: parking IS the
//! behaviour, so its deadline has to be LONGER than the longest park a caller
//! will ask for, or the driver gives up on a read the server is still
//! honouring. The driver's own default reply deadline is half a second, and a
//! connection opened without naming a longer one fails every `BLOCK 5000` at
//! 500 ms while the server keeps the socket parked for the remaining four and
//! a half. So [`Dedicated::connect`] takes the longest park the owner will
//! request, and the reply deadline is that park plus the role's
//! `request_timeout`: the server's bound, then the ordinary allowance for the
//! answer to travel. Still a bound — a peer that vanishes without closing the
//! socket is noticed, and `BLOCK 0` (wait forever) is refused by construction
//! because no park is declared for it.
//!
//! The DIAL is not governed by this allowance: [`crate::transport`] bounds
//! every attempt, so a peer that accepts a socket and then says nothing is
//! reported by the driver's own error, with its source chain intact.
//!
//! # A dropped socket heals, as the shared handle's does
//!
//! The driver redials a node whose socket died and re-reads the slot map on a
//! redirect; a command that meets a dead socket fails, and the next one goes
//! down the new socket. The ownership rule above is unchanged.

use std::time::Duration;

use redis::cluster_async::ClusterConnection;
use redis::{Cmd, FromRedisValue, Value};

use crate::config::{RedisConfig, RedisRole};
use crate::error::{self, Result};
use crate::transport;

/// A cluster connection with exactly one owner.
///
/// See the module note: this exists so a component may issue a command that
/// parks — and it is the type system, not a comment, that keeps a second
/// caller off the socket.
pub struct Dedicated {
    role: RedisRole,
    connection: ClusterConnection,
}

impl std::fmt::Debug for Dedicated {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Dedicated")
            .field("role", &self.role)
            .finish_non_exhaustive()
    }
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
    /// the cluster retries rather than failing a process that is otherwise
    /// healthy. Boot's promise that the cluster SERVES is made once, by the
    /// shared handle.
    ///
    /// # Errors
    /// Returns an unavailable error when the cluster cannot be reached within
    /// the role's `connect_timeout`, and a config error when a certificate
    /// authority file was named but not readable.
    pub async fn connect(config: &RedisConfig, longest_park: Duration) -> Result<Self> {
        let role = config.role().tag();
        let connection =
            transport::connect(config, longest_park + config.request_timeout()).await?;
        // Hoisted: see the `tracing` note in the workspace Cargo.toml.
        let park_ms = longest_park.as_millis();
        tracing::debug!(role, park_ms, event = "redis_dedicated_connected");
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

    /// Runs one command under the deadline the connection was opened with.
    ///
    /// `&mut self` rather than `&self`, which is the invariant stated as a
    /// signature: a second concurrent command on a socket that may be parked
    /// is the failure this type prevents, and here it does not compile.
    ///
    /// # Errors
    /// Returns a group-missing error for `NOGROUP`, an unavailable error when
    /// the connection dropped or the deadline passed, a command error
    /// otherwise, and an unexpected-reply error when the server answers a
    /// shape `T` cannot read.
    pub async fn command<T: FromRedisValue>(
        &mut self,
        name: &'static str,
        context: &str,
        cmd: &Cmd,
    ) -> Result<T> {
        let value = cmd
            .query_async::<Value>(&mut self.connection)
            .await
            .and_then(Value::extract_error)
            .map_err(|source| error::classify(name, context, source))?;
        // A parse failure is not a datastore failure — see [`crate::Redis::command`].
        T::from_redis_value(value).map_err(|_parse| error::unexpected_reply(name))
    }
}
