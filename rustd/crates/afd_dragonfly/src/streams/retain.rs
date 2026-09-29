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
//! producer can see, which a trim never was. How the floor is found, and why
//! no race moves it past owed work, is [`floor`]'s to say.
//!
//! # Where it runs
//!
//! On the acknowledgement path, gated on `XLEN`: a stream at or under the
//! history bound plus [`TRIM_SLACK`] costs one round trip per
//! acknowledgement, and a longer one pays the owed reads, one bounded range
//! read and the trim. Not a sweeper, because a sweep has to FIND the streams —
//! a walk of every fleet key per pass — while the acknowledgement already
//! holds the one stream that just grew.
//!
//! Generic over the key and the group rather than over `FleetStreams`, because
//! the outbound stream has the same shape and the same obligation, and one
//! floor computation is one place for the ordering to be right.

mod floor;

use redis::streams::{StreamInfoGroupsReply, StreamInfoStreamReply};

pub(crate) use self::floor::trim_history;
use super::{FLEET_CONSUMER_GROUP, FleetStreams, fleet_stream_key};
use crate::client::Dragonfly;
use crate::error::{self, Result};

/// The commands this module issues, named once each (RULE UFS).
const CMD_XLEN: &str = "XLEN";
const CMD_XINFO: &str = "XINFO";

/// `XINFO` subcommands.
const XINFO_STREAM: &str = "STREAM";
const XINFO_GROUPS: &str = "GROUPS";

/// How many acknowledged entries a stream keeps above its floor.
///
/// Nothing in the product reads acknowledged history off the stream — the
/// event history is `core.fleet_events`, and the live tail is pub/sub — so the
/// bound is a diagnostic window, not a promise. It is also the ceiling on
/// what a group restore can re-offer, so it stays small on purpose.
pub const ACKNOWLEDGED_HISTORY: usize = 1_000;

/// How far past [`ACKNOWLEDGED_HISTORY`] a stream grows before an
/// acknowledgement trims it back.
///
/// With no slack, every acknowledgement past the bound paid the whole trim —
/// the owed reads, a range read and an `XTRIM` — to remove one entry. A tenth
/// of the bound makes it one trim per 101 acknowledgements, removing 101
/// entries and reading 102, and a plain `XLEN` for the other hundred. The
/// price is at most a tenth more acknowledged history per stream, and each
/// entry carries its request payload, which is why the slack is not larger.
pub const TRIM_SLACK: usize = ACKNOWLEDGED_HISTORY / 10;

/// A stream entry's position: the two integers Dragonfly mints an id from.
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
    /// `None` when it cannot: Dragonfly reports the lag as nil after entries were
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
    /// Entries the floor read returned: at most `removed + 1`, and zero when
    /// the stream sat inside its bound and no read ran.
    pub read: u64,
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
async fn group_of(redis: &Dragonfly, key: &str, group: &str) -> Result<Option<Group>> {
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

/// How many entries `key` holds.
pub(crate) async fn length_of(redis: &Dragonfly, key: &str) -> Result<u64> {
    let mut cmd = redis::cmd(CMD_XLEN);
    cmd.arg(key);
    redis.command(CMD_XLEN, key, &cmd).await
}

/// What `group` still owes on `key`, or `None` when the group does not exist.
///
/// # Errors
/// Returns a command error when the stream cannot be described — including
/// when there is no such key, because a caller asking about a stream that
/// was never created has a different bug than one asking about an empty one.
pub(crate) async fn backlog_of(
    redis: &Dragonfly,
    key: &str,
    group: &str,
) -> Result<Option<Backlog>> {
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

    /// Trims the fleet's acknowledged history back to
    /// [`ACKNOWLEDGED_HISTORY`] once it is more than [`TRIM_SLACK`] past it,
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
    /// Returns a command error, or an unavailable error when Dragonfly is gone. A
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
