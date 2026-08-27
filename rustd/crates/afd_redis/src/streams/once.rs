//! Appending an event AT MOST ONCE, however many times the caller asks.
//!
//! The repair path retries through crash-shaped holes, so its append has to be
//! idempotent across process restarts — which makes this the one write here
//! that is a Lua script rather than a command, and the one that keeps a key of
//! its own. Split out because none of that is true of an ordinary append.

use super::{
    APPEND_ONCE, Appended, CMD_DEL, CMD_EVAL, EventId, FleetStreams, ONCE_KEY_PREFIX,
    OUTCOME_REPLAYED, STREAM_MAXLEN, fleet_stream_key,
};
use crate::error::{self, Result};

/// The key one append-once intent is remembered under.
///
/// One spelling (RULE UFS): `append_once` writes it and `forget_once` deletes
/// it, and a pair that drifted would leave the write remembered forever while
/// the delete removed nothing — a repair intent that could never run again.
fn once_key(once_id: &str) -> String {
    format!("{ONCE_KEY_PREFIX}{once_id}")
}

impl FleetStreams {
    /// Appends one event AT MOST ONCE, however many times this is called.
    ///
    /// The durable intent behind a repair verification is retried until the
    /// database records which event it produced, and those two writes cannot be
    /// one transaction — one is Redis and one is Postgres. So the retry has to
    /// be safe, and "safe" here means the second attempt returns the FIRST
    /// attempt's event id rather than appending a second event: a duplicate
    /// would run the same verification twice, with real provider spend.
    ///
    /// A `SET NX` beside the append would not do it — the two are separate
    /// round trips and a crash between them leaves either an event nothing
    /// remembers or a key naming no event. The script makes the pair atomic,
    /// which is the property the whole retry loop rests on.
    ///
    /// Answers the event id and whether this call is the one that wrote it.
    ///
    /// # Errors
    /// Returns a command error, or an unavailable error when Redis is gone. A
    /// key holding something that is not a stream is refused by the script
    /// rather than appended to.
    pub async fn append_once(
        &self,
        once_id: &str,
        fleet_id: &str,
        fields: &[(&str, &str)],
    ) -> Result<Appended> {
        let key = fleet_stream_key(fleet_id);
        let mut invocation = APPEND_ONCE.prepare_invoke();
        invocation
            .key(once_key(once_id))
            .key(&key)
            .arg(STREAM_MAXLEN);
        for (name, value) in fields {
            invocation.arg(*name).arg(*value);
        }

        let (event_id, outcome): (String, String) =
            self.redis.script(CMD_EVAL, &key, &invocation).await?;
        if event_id.is_empty() {
            return Err(error::unexpected_reply(CMD_EVAL));
        }
        Ok(Appended {
            id: EventId(event_id),
            replayed: outcome == OUTCOME_REPLAYED,
        })
    }

    /// Forgets an append-once key.
    ///
    /// Called only AFTER the database records which event the intent produced.
    /// Cleared any earlier and a retry in between would append a second event,
    /// which is the exact duplicate the key exists to prevent.
    ///
    /// # Errors
    /// Returns a command error when the delete fails.
    pub async fn forget_once(&self, once_id: &str) -> Result<()> {
        let key = once_key(once_id);
        let mut cmd = redis::cmd(CMD_DEL);
        cmd.arg(&key);
        let _removed: i64 = self.redis.command(CMD_DEL, &key, &cmd).await?;
        Ok(())
    }
}
