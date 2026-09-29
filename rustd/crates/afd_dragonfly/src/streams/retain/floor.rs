//! Where an acknowledgement may cut a stream, and the one read that finds it.
//!
//! # The floor
//!
//! The trim removes every entry below the least of three positions: the
//! group's last delivered id (everything after it is undelivered), its oldest
//! pending id (everything from it on is owed), and the oldest of the newest
//! `keep` entries (the history window). The first two together are the OWED
//! position. The third is found by reading from the oldest end.
//!
//! # The read stops at what is owed
//!
//! That read is `XRANGE key - <owed> COUNT n`, where `n` is how far the stream
//! is past `keep`, plus one. Ending at the owed position, it returns only
//! entries the trim may remove plus the one it stops at. A full reply's last
//! entry is the oldest of the newest `keep`; a short one read everything up to
//! the owed position, which is then the floor. The read this replaced walked
//! `keep` entries back from the newest end to use one id, on every
//! acknowledgement past the bound.
//!
//! [`TRIM_READ_MAX`] caps the read again, because every entry carries its
//! request payload. When a slow consumer's pinned entry clears, the whole
//! backlog above it becomes removable at once; one trim takes the first
//! [`TRIM_READ_MAX`] of it and the next acknowledgements take the rest.
//!
//! # Races keep owed work; a concurrent trim can shorten history
//!
//! Four reads and one `XTRIM MINID`, with no script. The floor never passes
//! the owed position, and the owed position only errs low: an append lands
//! above every id read, a delivery moves `last-delivered-id` forward and never
//! back, an acknowledgement raises the oldest pending id, and an entry joins a
//! pending list only by being delivered past `last-delivered-id`. Nothing the
//! trim removes was owed. That holds while nothing moves a group backwards —
//! no `XGROUP SETID` — and nothing puts an acknowledged entry back on a
//! pending list, which `XCLAIM … FORCE` would; the daemon issues neither.
//!
//! One race loosens the history window instead. The window is counted from the
//! oldest end, so a trim that ran between this one's `XLEN` and its read shifts
//! the window up, and acknowledged history can end shorter than `keep`. It
//! still stops at the owed position: the window is a diagnostic bound, and
//! owed work is the one thing this module must never remove. That is also why
//! a single-key Lua script, one round trip instead of five, was not worth its
//! unverified `XPENDING`-inside-a-script on Dragonfly.

use redis::streams::{StreamPendingReply, StreamRangeReply};

use super::{ACKNOWLEDGED_HISTORY, Position, TRIM_SLACK, Trimmed, as_u64, group_of, length_of};
use crate::client::Dragonfly;
use crate::error::{self, Result};
use crate::streams::{ARG_COUNT, CMD_XRANGE};

/// The commands this module issues, named once each (RULE UFS).
const CMD_XPENDING: &str = "XPENDING";
const CMD_XTRIM: &str = "XTRIM";

/// `XTRIM`'s strategy: remove every entry whose id is below the one given.
const XTRIM_MINID: &str = "MINID";

/// `XRANGE`'s lower bound: the oldest entry on the stream.
const RANGE_OLDEST: &str = "-";

/// The most entries one trim reads.
///
/// The history bound, so no trim reads more than the read it replaced did on
/// every acknowledgement past the bound.
const TRIM_READ_MAX: usize = ACKNOWLEDGED_HISTORY;

/// Trims `key`'s acknowledged history back to `keep` entries once it is more
/// than [`TRIM_SLACK`] past it, never crossing the oldest entry `group` still
/// owes.
///
/// # Errors
/// Returns a command error, or an unavailable error when the datastore is
/// gone. Nothing is trimmed on any error.
pub(crate) async fn trim_history(
    redis: &Dragonfly,
    key: &str,
    group: &str,
    keep: usize,
) -> Result<Trimmed> {
    let length = length_of(redis, key).await?;
    let untouched = Trimmed {
        removed: 0,
        retained: length,
        read: 0,
    };
    let Some(excess) = excess_of(length, keep) else {
        return Ok(untouched);
    };
    let Some(owed) = owed_position(redis, key, group).await? else {
        return Ok(untouched);
    };
    let wanted = window_size(excess);
    let window = oldest_through(redis, key, owed, wanted).await?;
    let read = as_u64(window.len());
    let Some(floor) = floor_of(owed, &window, wanted) else {
        return Ok(Trimmed { read, ..untouched });
    };

    let mut cmd = redis::cmd(CMD_XTRIM);
    cmd.arg(key).arg(XTRIM_MINID).arg(floor.render());
    let removed: u64 = redis.command(CMD_XTRIM, key, &cmd).await?;
    Ok(Trimmed {
        removed,
        retained: length.saturating_sub(removed),
        read,
    })
}

/// How far `length` is past `keep`, when it is past `keep` plus the slack.
fn excess_of(length: u64, keep: usize) -> Option<usize> {
    let keep = as_u64(keep);
    (length > keep.saturating_add(as_u64(TRIM_SLACK)))
        .then(|| usize::try_from(length.saturating_sub(keep)).unwrap_or(usize::MAX))
}

/// How many of the oldest entries the floor read asks for: the excess, capped
/// at [`TRIM_READ_MAX`], plus the one entry the trim stops at.
fn window_size(excess: usize) -> usize {
    excess.min(TRIM_READ_MAX).saturating_add(1)
}

/// The oldest position `group` still owes on `key`: the lesser of its last
/// delivered id and its oldest pending id. `None` when the stream has no such
/// group, where every entry is owed and nothing may be trimmed.
async fn owed_position(redis: &Dragonfly, key: &str, group: &str) -> Result<Option<Position>> {
    let Some(found) = group_of(redis, key, group).await? else {
        return Ok(None);
    };
    let pending = oldest_pending(redis, key, group).await?;
    Ok(Some(pending.map_or(found.last_delivered, |oldest| {
        oldest.min(found.last_delivered)
    })))
}

/// The oldest entry any consumer of `group` still holds, if one does.
async fn oldest_pending(redis: &Dragonfly, key: &str, group: &str) -> Result<Option<Position>> {
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

/// Up to `count` of the oldest entries at or below `through`, by position.
///
/// `XRANGE`'s end bound is inclusive, so an entry AT the owed position is
/// read, and kept: the floor is exclusive.
async fn oldest_through(
    redis: &Dragonfly,
    key: &str,
    through: Position,
    count: usize,
) -> Result<Vec<Position>> {
    let mut cmd = redis::cmd(CMD_XRANGE);
    cmd.arg(key)
        .arg(RANGE_OLDEST)
        .arg(through.render())
        .arg(ARG_COUNT)
        .arg(count);
    let reply: StreamRangeReply = redis.command(CMD_XRANGE, key, &cmd).await?;
    reply
        .ids
        .iter()
        .map(|entry| Position::parse(&entry.id))
        .collect()
}

/// Where the trim cuts, from the owed position and the oldest entries up to it.
///
/// A window holding all `wanted` entries ends at the oldest of the newest
/// `keep`, which is at or below `owed` by construction of the read. A shorter
/// one holds every entry up to `owed`, so `owed` is the floor. `None` when no
/// entry lies below the floor, and the `XTRIM` would remove nothing.
fn floor_of(owed: Position, window: &[Position], wanted: usize) -> Option<Position> {
    let floor = if window.len() >= wanted {
        window.last().copied()?
    } else {
        owed
    };
    window
        .first()
        .filter(|oldest| **oldest < floor)
        .map(|_below| floor)
}

#[cfg(test)]
#[path = "floor/tests.rs"]
mod tests;
