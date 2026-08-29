//! The three key-value operations the approval gate needs, and nothing else.
//!
//! # Why these live here and not at the call site
//!
//! `afd_fleet` names no Redis command, for the same reason it names no axum
//! type: the crate boundary IS the seam, and a `redis::cmd` built in a domain
//! module is that seam leaking. Every other Redis shape this daemon uses
//! already arrives through a typed surface — [`crate::ready::ReadyIndex`],
//! [`crate::streams::FleetStreams`], [`crate::session::SessionStore`] — and
//! these are the same thing for the gate.
//!
//! They are deliberately GENERIC rather than gate-shaped. A `get_gate_ref` here
//! would put the approval gate's vocabulary in the queue crate, and the key
//! shapes belong beside the sweeper and the resolver that also read them. What
//! this module knows is that a string went in and a string came out.

use crate::client::Redis;
use crate::error::Result;

/// Read one key.
const CMD_GET: &str = "GET";

/// Write one key with an expiry.
const CMD_SET: &str = "SET";

/// Increment a counter, setting its window on first touch.
const CMD_EVAL: &str = "EVAL";

/// Remove one key, answering how many were removed.
const CMD_DEL: &str = "DEL";

/// `SET`'s seconds-relative expiry argument.
const ARG_EXPIRE_SECONDS: &str = "EX";

/// How many keys the counter script takes.
const COUNTER_KEY_COUNT: &str = "1";

/// Increment, and set the window only on the first increment.
///
/// One script rather than `INCR` then `EXPIRE`, and the difference is not
/// tidiness. A crash between the two commands strands a freshly created key
/// with NO expiry: every later call then sees a count above one, skips the
/// expiry branch forever, and the counter accumulates without bound until it
/// crosses a threshold that was meant to describe a window — at which point it
/// auto-kills the fleet, permanently, for traffic spread over days.
const INCREMENT_IN_WINDOW: &str = r"
local count = redis.call('INCR', KEYS[1])
if count == 1 then redis.call('EXPIRE', KEYS[1], ARGV[1]) end
return count
";

impl Redis {
    /// The string at `key`, or `None` when nothing is stored there.
    ///
    /// # Errors
    /// Returns a command error when Redis will not answer, and an
    /// unexpected-reply error when the value is not a string.
    pub async fn get_string(&self, key: &str) -> Result<Option<String>> {
        let mut command = redis::cmd(CMD_GET);
        command.arg(key);
        self.command(CMD_GET, key, &command).await
    }

    /// Store `value` at `key` for `ttl_seconds`.
    ///
    /// # Errors
    /// Returns a command error when Redis will not answer, which includes a
    /// non-positive expiry — Redis rejects those rather than storing forever,
    /// and the caller is the one holding the arithmetic that produced it.
    pub async fn set_for(&self, key: &str, value: &str, ttl_seconds: i64) -> Result<()> {
        let mut command = redis::cmd(CMD_SET);
        command
            .arg(key)
            .arg(value)
            .arg(ARG_EXPIRE_SECONDS)
            .arg(ttl_seconds);
        let _stored: String = self.command(CMD_SET, key, &command).await?;
        Ok(())
    }

    /// Remove `key`, answering whether THIS caller is the one that removed it.
    ///
    /// The single-use primitive, and the reason it is a `DEL` rather than a read
    /// followed by a write: a `GET` that finds a key and a `DEL` that removes it
    /// are two commands with a gap, and two callers racing through that gap both
    /// see the key. `DEL` answers the count it removed, so exactly one caller can
    /// ever be told `true`.
    ///
    /// `false` for a key that was never there and for one that expired. Neither
    /// is an error: both mean the same thing to whoever asked, which is that the
    /// slot they were spending is gone.
    ///
    /// # Errors
    /// Returns a command error when Redis will not answer. Deliberately not
    /// collapsed into `false` — a store that is down would otherwise read as a
    /// slot somebody else already spent.
    pub async fn spend_key(&self, key: &str) -> Result<bool> {
        let mut command = redis::cmd(CMD_DEL);
        command.arg(key);
        let removed: i64 = self.command(CMD_DEL, key, &command).await?;
        Ok(removed == 1)
    }

    /// Increment the counter at `key`, giving it a `window_seconds` life on the
    /// first increment, and answer its new value.
    ///
    /// Atomic — see [`INCREMENT_IN_WINDOW`] for what the two-command version
    /// costs when it is interrupted.
    ///
    /// # Errors
    /// Returns a command error when Redis will not answer.
    pub async fn increment_in_window(&self, key: &str, window_seconds: u32) -> Result<i64> {
        let mut command = redis::cmd(CMD_EVAL);
        command
            .arg(INCREMENT_IN_WINDOW)
            .arg(COUNTER_KEY_COUNT)
            .arg(key)
            .arg(window_seconds);
        self.command(CMD_EVAL, key, &command).await
    }
}
