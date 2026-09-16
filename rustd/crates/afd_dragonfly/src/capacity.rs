//! What the datastore holds, counted by what it is for.
//!
//! One figure for "how full" would hide the only question an operator has:
//! full of WHAT. A stream backlog and a readiness backlog are different
//! incidents; retained history and pending work are different costs; a shard
//! with no replica is a different risk from a large one. So the sample keeps
//! every class apart and adds nothing up, and the ledger's own backlog —
//! Postgres rows, not keys — is reported beside it by the crate that owns
//! the ledger, never folded in here.
//!
//! # A bounded walk, and it says so
//!
//! Counting streams means scanning every primary for their keys, which is
//! what [`crate::Redis::scan_keys`] already does for sessions, and describing
//! each one is a command per stream. The walk is capped at what the caller
//! is willing to pay, and the report carries both how many streams exist and
//! how many were described, so a truncated sample reads as one instead of as
//! a small deployment.

use crate::client::Redis;
use crate::error::Result;
use crate::ready::{Partition, ReadyIndex};
use crate::streams::{FLEET_CONSUMER_GROUP, FLEET_STREAM_GLOB, retain};
use crate::topology::{self, Role};

/// How many keys one `SCAN` page asks for.
const SCAN_PAGE: usize = 1_000;

/// What the datastore holds, one figure per class of state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Capacity {
    /// Fleet streams found across every primary.
    pub streams: u64,
    /// Of those, how many were described; less than `streams` when the walk
    /// hit its cap.
    pub streams_walked: u64,
    /// Entries retained over the walked streams, owed or not.
    pub retained_entries: u64,
    /// Entries delivered and not acknowledged over the walked streams.
    pub pending_entries: u64,
    /// Readiness partitions HOLDING at least one mark, not the width the
    /// index declares — that width is a constant and describes no deployment.
    pub ready_partitions: u64,
    /// Fleets marked ready across every partition.
    pub ready_marks: u64,
    /// Primaries the cluster names.
    pub primaries: u64,
    /// Replicas the cluster names.
    pub replicas: u64,
}

impl Capacity {
    /// Samples the datastore, describing at most `walk_cap` streams.
    ///
    /// # Errors
    /// Returns a command error when a walk, a description or the topology
    /// read fails, and an unavailable error when the datastore is gone. A
    /// sample that cannot be completed is reported rather than filled in with
    /// zeros, because a zero here reads as an empty deployment.
    pub async fn sample(redis: &Redis, walk_cap: usize) -> Result<Self> {
        let keys = redis.scan_keys(FLEET_STREAM_GLOB, SCAN_PAGE).await?;
        let mut sample = Self {
            streams: count(keys.len()),
            ..Self::default()
        };
        for key in keys.iter().take(walk_cap) {
            sample.streams_walked += 1;
            sample.retained_entries += retain::length_of(redis, key).await?;
            if let Some(backlog) = retain::backlog_of(redis, key, FLEET_CONSUMER_GROUP).await? {
                sample.pending_entries += backlog.pending;
            }
        }

        let index = ReadyIndex::new(redis.clone());
        for partition in Partition::all() {
            // Counted only when the partition HOLDS something. Incrementing per
            // iteration reports the width the index declares, which is a
            // constant and tells a reader nothing; what the sample is for is
            // how far the marks have spread across that width.
            let marks = index.len_of(partition).await?;
            if marks > 0 {
                sample.ready_partitions += 1;
            }
            sample.ready_marks += marks;
        }

        for node in topology::nodes(redis).await? {
            match node.role {
                Role::Primary => sample.primaries += 1,
                Role::Replica => sample.replicas += 1,
            }
        }
        Ok(sample)
    }
}

/// A length in the width the report counts in.
fn count(length: usize) -> u64 {
    u64::try_from(length).unwrap_or(u64::MAX)
}
