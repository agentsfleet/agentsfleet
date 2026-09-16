//! Per-fleet event streams: append, read as a group, acknowledge.
//!
//! # The entry id is a RECEIPT, not the event id
//!
//! `XADD … *` makes Dragonfly mint the id, and that id addresses the ENTRY: it is
//! what `XACK` and a claim take. The event's identity is the admission
//! ledger's logical id, which the producer writes into the entry's `event_id`
//! field — and after a replay one logical event can have had two entries, so
//! the two are not interchangeable. [`EventId`] says the receipt half in the
//! type system: it is produced by an append and consumed by an
//! acknowledgement, so a logical id cannot be passed to `XACK` by accident.
//!
//! # A missing group is restored by its reader, at a cursor the ledgers prove
//!
//! The group is created on the write path, so the steady state here is a plain
//! read with no setup command in front of it. It can still vanish — deleted out
//! of band, a restart without persistence, a failover to an empty replica — and
//! every one of those announces itself the same way: `NOGROUP` on the next read.
//!
//! This crate reports that and does not repair it, because it cannot know
//! WHERE to. The two blind choices are both wrong: at `0`, every retained
//! entry is handed out again and the lease path re-runs each one — historical
//! agent runs re-executing with real provider spend and real connector writes,
//! since a redelivered entry is still a run (it merely skips the receive
//! debit); at `$`, every entry appended during the groupless window is lost,
//! which is accepted work vanishing. The reader that holds the durable
//! ledgers asks them for the newest receipt that was DELIVERED and calls
//! [`FleetStreams::restore_group`] there: everything after it is undelivered
//! and is offered, everything at or before it ran, and nothing is guessed.
//!
//! # Retention is bounded by unfinished work, not by a length
//!
//! An append carries no `MAXLEN`. The old `MAXLEN ~ 10000` trimmed the
//! oldest entries whatever their state, so a consumer ten thousand entries
//! behind lost work it had never been handed, on the append path of a
//! producer that was told yes. Trimming lives in [`retain`], runs on the
//! acknowledgement path, and never crosses the oldest pending or undelivered
//! entry; the admission budget is what bounds a stream that is not draining.

pub(crate) mod render;
pub(crate) mod retain;
mod tail;

#[cfg(feature = "test-util")]
pub use self::render::rendered_field_samples;
use self::render::stringify;
pub use self::retain::{ACKNOWLEDGED_HISTORY, Backlog, Trimmed};

use crate::client::Dragonfly;
use crate::error::{self, Result};

/// The commands this module issues, named once each (RULE UFS).
const CMD_XADD: &str = "XADD";
const CMD_XGROUP: &str = "XGROUP";

/// Asks what a key holds, so a create is never issued over the wrong thing.
const CMD_TYPE: &str = "TYPE";

/// `TYPE`'s answer for a key that does not exist.
const TYPE_NONE: &str = "none";

/// `TYPE`'s answer for a key that is already a stream.
const TYPE_STREAM: &str = "stream";
const CMD_XREADGROUP: &str = "XREADGROUP";
const CMD_XACK: &str = "XACK";
const CMD_XAUTOCLAIM: &str = "XAUTOCLAIM";
const CMD_XRANGE: &str = "XRANGE";

/// The cap argument `XRANGE`, `XREVRANGE` and `XAUTOCLAIM` all take.
pub(crate) const ARG_COUNT: &str = "COUNT";
const CMD_DEL: &str = "DEL";

/// Consumer group every fleet stream is read under.
pub const FLEET_CONSUMER_GROUP: &str = "fleet_lease";

/// How long an entry must have sat undelivered before it may be claimed away
/// from the consumer holding it.
///
/// Comfortably past the lease window, which is what stops the sweep racing live
/// work: a consumer still working an entry has not been idle this long, and one
/// that has is a retired instance or a legacy throwaway consumer name.
const AUTOCLAIM_MIN_IDLE_MS: usize = 300_000;

/// Where an autoclaim scan starts, and how many entries it takes.
///
/// Always from the beginning of the pending list: a claimed entry's idle clock
/// RESETS, so the same entry is not eligible twice and the scan makes progress
/// without a cursor to carry.
const AUTOCLAIM_START: &str = "0-0";

/// Read id meaning "entries never delivered to any consumer".
const NEW_ENTRIES: &str = ">";

/// Read id meaning "this consumer's own pending entries, oldest first".
const OWN_PENDING: &str = "0";

/// Group start id for a stream that is brand new, where "from the beginning"
/// and "from now" are the same position — and for a restore that found
/// nothing delivered, where every retained entry is still owed.
const GROUP_START_BEGIN: &str = "0";

/// Where a restored consumer group starts delivering from.
///
/// Decided by the reader that holds the durable ledgers, never by this crate:
/// the position is a fact about what RAN, and only Postgres knows that. The
/// two arms are the two answers the ledgers can give; there is deliberately no
/// third for `$`, because "skip whatever is there" is never a position the
/// ledgers would name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GroupCursor {
    /// This receipt and everything before it were delivered; deliver what
    /// follows.
    After(EventId),
    /// Nothing on this stream was ever delivered; deliver from the beginning.
    Beginning,
}

/// The key a fleet's events live on.
#[must_use]
pub fn fleet_stream_key(fleet_id: &str) -> String {
    format!("fleet:{fleet_id}:events")
}

/// The pattern every fleet stream key matches and nothing else does — the
/// same shape as [`fleet_stream_key`], with the fleet left open.
pub(crate) const FLEET_STREAM_GLOB: &str = "fleet:*:events";

/// The channel a fleet's live-tail frames are published on.
///
/// `activity_publisher.zig` builds this into a 128-byte stack buffer and has a
/// failure arm for the overflow, which is a fact about `bufPrint` rather than
/// about the channel. Here the string owns its own length and the arm is gone —
/// there is no truncated-channel case left to handle, so nothing has to decide
/// what publishing to a truncated channel would mean.
///
/// Distinct from [`fleet_stream_key`] and deliberately adjacent to it: one is a
/// durable STREAM that survives a restart and one is a pub/sub CHANNEL with no
/// retention at all, and a caller reaching for the wrong one would either lose
/// every frame or persist cosmetic ones forever.
#[must_use]
pub fn fleet_activity_channel(fleet_id: &str) -> String {
    format!("fleet:{fleet_id}:activity")
}

/// A Dragonfly stream entry id: the receipt an append answers with, and the only
/// thing `XACK` and a claim accept.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EventId(String);

impl EventId {
    /// The receipt as Dragonfly spelled it, `{millis}-{sequence}`.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// A receipt read back out of a column rather than minted by an append.
    ///
    /// The report path needs this: the entry was acknowledged long after the
    /// poll that read it, by a different request, and what survives in between
    /// is the `fleet.runner_leases.receipt` text. Deliberately not a `From`
    /// impl — a receipt is a thing Dragonfly produced, and a blanket conversion
    /// from `&str` would let any string in the program become one silently.
    #[must_use]
    pub fn of(stored: &str) -> Self {
        Self(stored.to_owned())
    }
}

impl std::fmt::Display for EventId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// One event read off a fleet stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FleetEvent {
    /// The entry id — the receipt this delivery is acknowledged by.
    ///
    /// Named `receipt` and not `id` on purpose: the event's identity is the
    /// `event_id` FIELD the admission ledger wrote, and after a replay the
    /// two differ. A reader reaching for `.id` and getting the entry would
    /// key billing on a value a replay can change.
    pub receipt: EventId,
    /// The entry's fields, in the order Dragonfly returned them.
    pub fields: Vec<(String, String)>,
}

impl FleetEvent {
    /// The value of one field, if the entry carries it.
    #[must_use]
    pub fn field(&self, name: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }
}

/// Fleet stream operations against one connection.
#[derive(Debug, Clone)]
pub struct FleetStreams {
    redis: Dragonfly,
}

mod consume;

impl FleetStreams {
    /// Binds stream operations to a connection.
    #[must_use]
    pub const fn new(redis: Dragonfly) -> Self {
        Self { redis }
    }

    /// Creates the consumer group, delivering from the stream's beginning.
    ///
    /// Idempotent: an existing group answers `BUSYGROUP`, which is the expected
    /// steady state and not a failure. `MKSTREAM` so the first call on a fleet
    /// that has never had an event still leaves a group behind.
    ///
    /// # Errors
    /// Returns a command error when the group could not be created for any
    /// reason other than already existing.
    pub async fn ensure_group(&self, fleet_id: &str) -> Result<()> {
        self.create_group(fleet_id, GROUP_START_BEGIN).await
    }

    /// Recreates a vanished consumer group at `cursor`.
    ///
    /// The other half of the `NOGROUP` a read reports — see the module note on
    /// why the read does not do this itself. Idempotent for the same reason
    /// [`FleetStreams::ensure_group`] is: two readers restoring one fleet at
    /// once both succeed, and the second's cursor is discarded because the
    /// first's was computed from the same ledgers.
    ///
    /// # Errors
    /// Returns a command error when the group could not be created for any
    /// reason other than already existing.
    pub async fn restore_group(&self, fleet_id: &str, cursor: &GroupCursor) -> Result<()> {
        let start = match cursor {
            GroupCursor::After(receipt) => receipt.as_str(),
            GroupCursor::Beginning => GROUP_START_BEGIN,
        };
        self.create_group(fleet_id, start).await
    }

    /// Refuses a create the server cannot be asked to refuse for us.
    ///
    /// `MKSTREAM` below is what makes `XGROUP CREATE` create its key, and a
    /// create reaches `DbSlice::AddNew`. Over a key already holding another
    /// type, Dragonfly v1.40.2 trips `db_slice.cc:1176 Check failed: res.is_new`
    /// and aborts the node instead of answering `WRONGTYPE`, taking every other
    /// caller on that node down with it. Dragonfly answers `WRONGTYPE`, so the
    /// guard is this datastore's, not the protocol's.
    ///
    /// `none` and `stream` are both fine: the first is what `MKSTREAM` exists
    /// for, and the second answers `BUSYGROUP`, which the callers treat as
    /// success. Anything else never reaches the wire.
    async fn refuse_occupied_key(&self, key: &str) -> Result<()> {
        let mut cmd = redis::cmd(CMD_TYPE);
        cmd.arg(key);
        let holds: String = self.redis.command(CMD_TYPE, key, &cmd).await?;
        if holds == TYPE_NONE || holds == TYPE_STREAM {
            return Ok(());
        }
        Err(error::wrong_type(CMD_XGROUP, key, &holds))
    }

    async fn create_group(&self, fleet_id: &str, start: &str) -> Result<()> {
        let key = fleet_stream_key(fleet_id);
        self.refuse_occupied_key(&key).await?;
        let mut cmd = redis::cmd(CMD_XGROUP);
        cmd.arg("CREATE")
            .arg(&key)
            .arg(FLEET_CONSUMER_GROUP)
            .arg(start)
            .arg("MKSTREAM");

        match self.redis.command::<String>(CMD_XGROUP, &key, &cmd).await {
            Ok(_) => Ok(()),
            Err(failure) if failure.is_group_exists() => Ok(()),
            Err(failure) => Err(failure),
        }
    }

    /// Appends an event, returning the id Dragonfly minted for it.
    ///
    /// No `MAXLEN`: the append never trims, because an append cannot know
    /// what the consumer still owes. Retention is [`FleetStreams::trim`]'s,
    /// on the acknowledgement path, bounded below by unfinished work.
    ///
    /// # Errors
    /// Returns a command error when the append fails, a full error when the
    /// datastore refuses to grow, and an unexpected-reply error when Dragonfly
    /// answers with something that is not an id.
    pub async fn append(&self, fleet_id: &str, fields: &[(&str, &str)]) -> Result<EventId> {
        let key = fleet_stream_key(fleet_id);
        let mut cmd = redis::cmd(CMD_XADD);
        cmd.arg(&key).arg("*");
        for (name, value) in fields {
            cmd.arg(*name).arg(*value);
        }

        let id: String = self.redis.command(CMD_XADD, &key, &cmd).await?;
        if id.is_empty() {
            return Err(error::unexpected_reply(CMD_XADD));
        }
        tracing::debug!(fleet_id, receipt = %id, event = "xadd_fleet_event");
        Ok(EventId(id))
    }

    /// Drops a fleet's whole event stream, group and all.
    ///
    /// For the purge, and only for it: `DEL` on a stream key removes the
    /// entries AND every consumer group over them, which is correct exactly
    /// when the fleet itself is gone from Postgres and wrong at every other
    /// moment. A live fleet losing its group here would go quiet with nothing
    /// reporting why, because `ensure_group` only runs at install.
    ///
    /// Idempotent — a stream that is already gone removes nothing and succeeds,
    /// so a retried purge is not an error.
    ///
    /// # Errors
    /// Returns a command error, or an unavailable error when Dragonfly is gone.
    /// The purge logs and continues: Postgres has already committed, and the
    /// keys left behind are unreachable rather than harmful.
    pub async fn forget(&self, fleet_id: &str) -> Result<()> {
        let key = fleet_stream_key(fleet_id);
        let mut cmd = redis::cmd(CMD_DEL);
        cmd.arg(&key);
        let _removed: i64 = self.redis.command(CMD_DEL, &key, &cmd).await?;
        Ok(())
    }
}
