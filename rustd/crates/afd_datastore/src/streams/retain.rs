//! Retention, and the question it turns on: what a stream still owes.
//!
//! # The floor is unfinished work, not a length
//!
//! `XADD … MAXLEN ~ N` trims the oldest entries whatever their state, so a
//! consumer that fell N entries behind lost work it had never been handed —
//! silently, on the append path of a producer that was told yes. Retention
//! here is bounded BELOW by the oldest entry a consumer still owes: pending
//! (delivered, unacknowledged) or undelivered. Only the acknowledged history
//! above that floor is capped, at [`ACKNOWLEDGED_HISTORY`]. A slow consumer
//! therefore grows its stream instead of losing its work, and the admission
//! budget in `afd_admission` is what stops that growth — with a refusal the
//! producer can see, which a trim never was.
//!
//! # Every race makes the trim more conservative
//!
//! The floor is computed from three reads and applied by one `XTRIM MINID`,
//! with no script. Between the reads and the trim an append lands above every
//! id read, a delivery moves `last-delivered-id` forward and never back, and
//! an acknowledgement raises the oldest pending id. Each leaves the computed
//! floor at or below the true one, so the trim removes a subset of what it
//! could have. Nothing it removes was owed. That is why a single-key Lua
//! script — one round trip instead of five — was not worth its unverified
//! `XPENDING`-inside-a-script on Dragonfly.
//!
//! # Where it runs
//!
//! On the acknowledgement path, gated on `XLEN`: a stream at or under the
//! history bound costs one round trip per acknowledgement, and a longer one
//! pays the three reads and the trim. Not a sweeper, because a sweep has to
//! FIND the streams — a walk of every fleet key per pass — while the
//! acknowledgement already holds the one stream that just grew.
//!
//! Generic over the key and the group rather than over `FleetStreams`, because
//! the outbound stream has the same shape and the same obligation, and one
//! floor computation is one place for the ordering to be right.

use redis::streams::{
    StreamInfoGroupsReply, StreamInfoStreamReply, StreamPendingReply, StreamRangeReply,
};

use super::{FLEET_CONSUMER_GROUP, FleetStreams, fleet_stream_key};
use crate::client::Redis;
use crate::error::{self, Result};

/// The commands this module issues, named once each (RULE UFS).
const CMD_XLEN: &str = "XLEN";
const CMD_XINFO: &str = "XINFO";
const CMD_XPENDING: &str = "XPENDING";
const CMD_XREVRANGE: &str = "XREVRANGE";
const CMD_XTRIM: &str = "XTRIM";

/// `XINFO` subcommands.
const XINFO_STREAM: &str = "STREAM";
const XINFO_GROUPS: &str = "GROUPS";

/// `XTRIM`'s strategy: remove every entry whose id is below the one given.
const XTRIM_MINID: &str = "MINID";

/// `XREVRANGE`'s bounds, newest to oldest, and its cap.
const RANGE_NEWEST: &str = "+";
const RANGE_OLDEST: &str = "-";
const ARG_COUNT: &str = "COUNT";

/// How many acknowledged entries a stream keeps above its floor.
///
/// Nothing in the product reads acknowledged history off the stream — the
/// event history is `core.fleet_events`, and the live tail is pub/sub — so the
/// bound is a diagnostic window, not a contract. It is also the ceiling on
/// what a group restore can re-offer, so it stays small on purpose.
pub const ACKNOWLEDGED_HISTORY: usize = 1_000;

/// A stream entry's position: the two integers Redis mints an id from.
///
/// Parsed once, at the boundary, so ordering here is integer ordering and
/// never the lexical ordering of the text — under which `999-0` sorts after
/// `1000-0` and a floor would land in the wrong decade. Malformed text is
/// refused rather than ordered somewhere by accident.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Position {
    millis: u64,
    sequence: u64,
}

impl Position {
    /// Reads `{millis}-{sequence}`, refusing anything else as a reply this
    /// client does not understand.
    fn parse(id: &str) -> Result<Self> {
        id.split_once('-')
            .and_then(|(millis, sequence)| {
                Some(Self {
                    millis: millis.parse().ok()?,
                    sequence: sequence.parse().ok()?,
                })
            })
            .ok_or_else(|| error::unexpected_reply(CMD_XINFO))
    }

    /// The id as `XTRIM` takes it.
    fn render(self) -> String {
        format!("{}-{}", self.millis, self.sequence)
    }
}

/// What a consumer group still owes on its stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Backlog {
    /// Entries delivered to a consumer and not yet acknowledged.
    pub pending: u64,
    /// Entries no consumer has been handed, when the server can count them.
    ///
    /// `None` when it cannot: Redis reports the lag as nil after entries were
    /// deleted from the middle of the stream, and a count it will not vouch
    /// for is not one this type will invent.
    pub undelivered: Option<u64>,
}

impl Backlog {
    /// Pending plus undelivered, when both are known.
    ///
    /// The number an admission budget compares against. `None` is "unknown",
    /// which a budget admits through: refusing work on a figure the server
    /// declined to give would be a false refusal, and the retention floor
    /// keeps the work safe either way.
    #[must_use]
    pub fn outstanding(self) -> Option<u64> {
        self.undelivered
            .map(|undelivered| self.pending.saturating_add(undelivered))
    }

    /// Whether a runner could still pick something up here.
    ///
    /// An unknown undelivered count reads as deliverable. The direction
    /// matters and only one of them is safe: a false positive costs one
    /// wasted candidate check, and a false negative strands an event.
    #[must_use]
    pub fn is_deliverable(self) -> bool {
        self.pending > 0 || self.undelivered.is_none_or(|undelivered| undelivered > 0)
    }
}

/// What one trim did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Trimmed {
    /// Entries removed.
    pub removed: u64,
    /// Entries still on the stream afterwards.
    pub retained: u64,
}

/// The group's state as `XINFO GROUPS` reports it — the public backlog plus
/// the position the trim floor needs.
struct Group {
    backlog: Backlog,
    last_delivered: Position,
}

/// The named group on `key`, or `None` when the stream has no such group.
///
/// A missing group means no consumer has ever read: every entry is owed and
/// nothing may be trimmed, which is what every caller does with `None`.
async fn group_of(redis: &Redis, key: &str, group: &str) -> Result<Option<Group>> {
    let mut cmd = redis::cmd(CMD_XINFO);
    cmd.arg(XINFO_GROUPS).arg(key);
    let reply: StreamInfoGroupsReply = redis.command(CMD_XINFO, key, &cmd).await?;
    reply
        .groups
        .into_iter()
        .find(|found| found.name == group)
        .map(|found| {
            Ok(Group {
                backlog: Backlog {
                    pending: as_u64(found.pending),
                    undelivered: found.lag.map(as_u64),
                },
                last_delivered: Position::parse(&found.last_delivered_id)?,
            })
        })
        .transpose()
}

/// The oldest entry any consumer of `group` still holds, if one does.
async fn oldest_pending(redis: &Redis, key: &str, group: &str) -> Result<Option<Position>> {
    let mut cmd = redis::cmd(CMD_XPENDING);
    cmd.arg(key).arg(group);
    let reply: StreamPendingReply = redis.command(CMD_XPENDING, key, &cmd).await?;
    match reply {
        StreamPendingReply::Empty => Ok(None),
        StreamPendingReply::Data(summary) => Position::parse(&summary.start_id).map(Some),
        // A shape a newer driver added. Refused rather than read as "nothing
        // pending": the floor would then cross entries a consumer still owes,
        // which is the one thing this module exists to never do.
        _unknown => Err(error::unexpected_reply(CMD_XPENDING)),
    }
}

/// The oldest of the newest `keep` entries — the floor of the history window.
async fn history_floor(redis: &Redis, key: &str, keep: usize) -> Result<Option<Position>> {
    let mut cmd = redis::cmd(CMD_XREVRANGE);
    cmd.arg(key)
        .arg(RANGE_NEWEST)
        .arg(RANGE_OLDEST)
        .arg(ARG_COUNT)
        .arg(keep);
    let reply: StreamRangeReply = redis.command(CMD_XREVRANGE, key, &cmd).await?;
    reply
        .ids
        .last()
        .map(|entry| Position::parse(&entry.id))
        .transpose()
}

/// How many entries `key` holds.
pub(crate) async fn length_of(redis: &Redis, key: &str) -> Result<u64> {
    let mut cmd = redis::cmd(CMD_XLEN);
    cmd.arg(key);
    redis.command(CMD_XLEN, key, &cmd).await
}

/// Trims `key`'s acknowledged history to `keep` entries, never crossing the
/// oldest entry `group` still owes.
///
/// The floor is the least of three positions: the group's last delivered id
/// (everything after it is undelivered), the oldest pending id (everything
/// from it on is owed), and the oldest of the newest `keep` entries (the
/// history window). `XTRIM MINID` then removes what lies below.
pub(crate) async fn trim_history(
    redis: &Redis,
    key: &str,
    group: &str,
    keep: usize,
) -> Result<Trimmed> {
    let length = length_of(redis, key).await?;
    let untouched = Trimmed {
        removed: 0,
        retained: length,
    };
    if length <= as_u64(keep) {
        return Ok(untouched);
    }
    let Some(found) = group_of(redis, key, group).await? else {
        return Ok(untouched);
    };
    let floor = [
        Some(found.last_delivered),
        oldest_pending(redis, key, group).await?,
        history_floor(redis, key, keep).await?,
    ]
    .into_iter()
    .flatten()
    .min();
    let Some(floor) = floor else {
        return Ok(untouched);
    };

    let mut cmd = redis::cmd(CMD_XTRIM);
    cmd.arg(key).arg(XTRIM_MINID).arg(floor.render());
    let removed: u64 = redis.command(CMD_XTRIM, key, &cmd).await?;
    Ok(Trimmed {
        removed,
        retained: length.saturating_sub(removed),
    })
}

/// What `group` still owes on `key`, or `None` when the group does not exist.
///
/// # Errors
/// Returns a command error when the stream cannot be described — including
/// when there is no such key, because a caller asking about a stream that
/// was never created has a different bug than one asking about an empty one.
pub(crate) async fn backlog_of(redis: &Redis, key: &str, group: &str) -> Result<Option<Backlog>> {
    Ok(group_of(redis, key, group)
        .await?
        .map(|found| found.backlog))
}

/// A count the driver reports as `usize`, in the width the rest of the crate
/// counts in. Lossless on every target this daemon builds for.
fn as_u64(count: usize) -> u64 {
    u64::try_from(count).unwrap_or(u64::MAX)
}

impl FleetStreams {
    /// What the fleet's consumer group still owes, or `None` when the group
    /// does not exist yet.
    ///
    /// # Errors
    /// As [`backlog_of`].
    pub async fn backlog(&self, fleet_id: &str) -> Result<Option<Backlog>> {
        backlog_of(
            &self.redis,
            &fleet_stream_key(fleet_id),
            FLEET_CONSUMER_GROUP,
        )
        .await
    }

    /// Trims the fleet's acknowledged history to [`ACKNOWLEDGED_HISTORY`],
    /// never crossing the oldest entry the group still owes.
    ///
    /// # Errors
    /// Returns a command error, or an unavailable error when the datastore
    /// is gone. Callers on the acknowledgement path log and continue: the
    /// acknowledgement already landed, and the next one trims again.
    pub async fn trim(&self, fleet_id: &str) -> Result<Trimmed> {
        trim_history(
            &self.redis,
            &fleet_stream_key(fleet_id),
            FLEET_CONSUMER_GROUP,
            ACKNOWLEDGED_HISTORY,
        )
        .await
    }

    /// Whether this fleet holds work a runner could still pick up.
    ///
    /// The backstop for a readiness mark that was lost — an ingress mark that
    /// failed, an index that was evicted or flushed. The streams are the system
    /// of record and the index is a hint, so this asks the record.
    ///
    /// # Errors
    /// Returns a command error, or an unavailable error when Redis is gone. A
    /// probe that cannot answer is REPORTED rather than read as "nothing to
    /// recover" — this is the recovery path's own backstop, and a silent false
    /// would leave it inert while looking exactly like an idle system.
    pub async fn has_deliverable(&self, fleet_id: &str) -> Result<bool> {
        let key = fleet_stream_key(fleet_id);
        let mut stream_info = redis::cmd(CMD_XINFO);
        stream_info.arg(XINFO_STREAM).arg(&key);
        let stream: StreamInfoStreamReply =
            self.redis.command(CMD_XINFO, &key, &stream_info).await?;
        // No entries ever generated, so nothing to deliver whatever the group
        // says about itself.
        if stream.length == 0 {
            return Ok(false);
        }
        // No consumer group yet: no runner has ever read this fleet, so every
        // entry present is undelivered.
        Ok(self
            .backlog(fleet_id)
            .await?
            .is_none_or(Backlog::is_deliverable))
    }
}

#[cfg(test)]
#[path = "retain/tests.rs"]
mod tests;
