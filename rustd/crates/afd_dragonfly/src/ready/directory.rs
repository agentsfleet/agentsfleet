//! The readiness directory: which partitions gained a mark since a poll last
//! spent them.
//!
//! # Why it exists
//!
//! A poll reads one partition per rotation step, so a mark in a partition the
//! cursor just passed waits for the rotation to come round: sixteen polls, up
//! to four seconds with four runner workers on the one-second back-off. The
//! directory names the partitions that hold fresh marks, so the next poll
//! peeks there first, whichever partition its cursor names.
//!
//! # Why one hash, and why the entry carries the mark's token
//!
//! One key, so a poll learns every dirty partition in one command and a delete
//! is a script over one key, which runs on a cluster unchanged. A delete that
//! also looked inside the partition's hash would touch two slots and be
//! refused as `CROSSSLOT`. So the entry carries the token of the last mark
//! that named its partition, and a poll deletes the entry only while it still
//! carries the token the poll read: a mark landing in between rewrites the
//! token, and the delete leaves it.
//!
//! # Why every command has a deadline
//!
//! Every poll reads this one slot first. Under the datastore's own request
//! timeout a sick node here would stall every poll on every runner for
//! seconds, so each command gives up at `READY_HINT_DEADLINE_MS`. Giving up
//! costs speed and nothing else: the rotation still visits every partition,
//! and the mark the entry points at is already in its partition's hash.

use std::collections::HashMap;
use std::time::Duration;

use afd_core::error_code::{self, Coded as _, Logged};
use afd_core::timing::READY_HINT_DEADLINE_MS;
use afd_observability::metrics::label::fleet::ReadyHint;

use super::{
    CLEAR_IF_TOKEN_MATCHES_SCRIPT, CMD_EVAL, CMD_HRANDFIELD, CMD_HSET, Partition, Ready,
    ReadyIndex, ReadyToken,
};
use crate::error::{self, Result};

/// The directory read: every entry at once, at most one per partition.
const CMD_HGETALL: &str = "HGETALL";

/// The `event` a mark logs when its directory entry was not written.
const EVENT_HINT_WRITE_FAILED: &str = "ready_hint_write_failed";

/// [`READY_HINT_DEADLINE_MS`] as the duration a timeout takes.
const HINT_DEADLINE: Duration = Duration::from_millis(READY_HINT_DEADLINE_MS);

/// One partition the directory names, and the token of the last mark that
/// named it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dirty {
    /// The partition holding a fresh mark.
    pub partition: Partition,
    /// The token to pass back to [`ReadyIndex::spend`].
    pub token: ReadyToken,
}

impl ReadyIndex {
    /// The key this index's directory lives under.
    fn directory_key(&self) -> String {
        self.redis.physical_key(&self.prefix.directory_key())
    }

    /// Names `partition` dirty under `token`, the token of the mark that just
    /// landed there.
    ///
    /// Called by [`ReadyIndex::mark`] after its mark is written, and never
    /// raised from there: the mark is the record a poll needs, and a lost
    /// entry costs that fleet the rotation's speed, not its work. So a
    /// failure is reported here, once, with the partition it concerned.
    pub(super) async fn note_dirty(&self, partition: Partition, token: &ReadyToken) {
        let key = self.directory_key();
        let mut cmd = redis::cmd(CMD_HSET);
        cmd.arg(&key).arg(partition.index()).arg(token.as_str());
        let written = within(
            HINT_DEADLINE,
            CMD_HSET,
            self.redis.command::<i64>(CMD_HSET, &key, &cmd),
        )
        .await;
        if let Err(error) = written {
            report_unwritten(partition, &error);
        }
    }

    /// Every dirty partition, oldest mark first.
    ///
    /// Oldest first because the tokens are version-7 identifiers, which sort
    /// by the instant they were minted: two partitions dirtied in turn are
    /// served in that turn.
    ///
    /// # Errors
    /// Returns a timeout error past the hint deadline, and a command error
    /// when the read fails. A poll treats either as an empty directory.
    pub async fn dirty(&self) -> Result<Vec<Dirty>> {
        let key = self.directory_key();
        let mut cmd = redis::cmd(CMD_HGETALL);
        cmd.arg(&key);
        let entries: HashMap<String, String> = within(
            HINT_DEADLINE,
            CMD_HGETALL,
            self.redis.command(CMD_HGETALL, &key, &cmd),
        )
        .await?;
        Ok(oldest_first(entries))
    }

    /// Samples the partition the directory named, as [`ReadyIndex::peek`]
    /// does, but under the hint deadline.
    ///
    /// Every poll that reads the directory is sent to the same oldest entry,
    /// so a peek waiting out the datastore's request timeout there would stall
    /// every poll on every runner, where a sick partition reached by the
    /// rotation stalls one poll in sixteen.
    ///
    /// # Errors
    /// Returns a timeout error past the hint deadline, and a command error
    /// when the read fails.
    pub async fn peek_hinted(&self, partition: Partition, count: usize) -> Result<Vec<Ready>> {
        within(HINT_DEADLINE, CMD_HRANDFIELD, self.peek(partition, count)).await
    }

    /// Deletes `dirty`'s entry, but only if no mark has named its partition
    /// since it was read.
    ///
    /// Returns whether the entry was removed. `false` means a newer mark
    /// rewrote it, which is the case the token exists for.
    ///
    /// # Errors
    /// Returns a timeout error past the hint deadline, and a command error
    /// when the evaluation fails. A stale entry left behind costs one more
    /// peek of an empty partition.
    pub async fn spend(&self, dirty: &Dirty) -> Result<bool> {
        let key = self.directory_key();
        let mut invocation = CLEAR_IF_TOKEN_MATCHES_SCRIPT.prepare_invoke();
        invocation
            .key(&key)
            .arg(dirty.partition.index())
            .arg(dirty.token.as_str());
        let removed: i64 = within(
            HINT_DEADLINE,
            CMD_EVAL,
            self.redis.script(CMD_EVAL, &key, &invocation),
        )
        .await?;
        Ok(removed > 0)
    }
}

/// The directory's entries as [`Dirty`] values, oldest mark first.
///
/// A field that names no partition is skipped rather than refused: the
/// directory is a hint, and an entry nothing can peek is one the rotation
/// already covers.
fn oldest_first(entries: HashMap<String, String>) -> Vec<Dirty> {
    let mut dirty: Vec<Dirty> = entries
        .into_iter()
        .filter_map(|(field, token)| {
            field
                .parse::<u16>()
                .ok()
                .and_then(Partition::new)
                .map(|partition| Dirty {
                    partition,
                    token: ReadyToken(token),
                })
        })
        .collect();
    dirty.sort_by(|left, right| left.token.cmp(&right.token));
    dirty
}

/// `hint`, or a timeout error naming `command` once `deadline` passes.
async fn within<T>(
    deadline: Duration,
    command: &'static str,
    hint: impl Future<Output = Result<T>>,
) -> Result<T> {
    tokio::time::timeout(deadline, hint)
        .await
        .map_err(|_elapsed| error::timed_out(command, deadline.as_millis()))?
}

/// Reports a mark whose directory entry was not written.
fn report_unwritten(partition: Partition, failure: &crate::Error) {
    afd_observability::producers::fleet::ready_hint_failed(ReadyHint::Write);
    let code = error_code::INTERNAL_OPERATION_FAILED.as_str();
    let index = partition.index();
    let Logged {
        error_code: cause_error_code,
        reason,
    } = failure.logged();
    tracing::warn!(
        error_code = code,
        cause_error_code,
        event = EVENT_HINT_WRITE_FAILED,
        partition = index,
        reason,
        "a mark landed without its directory entry; the rotation still finds it"
    );
}

#[cfg(test)]
#[path = "directory/tests.rs"]
mod tests;
