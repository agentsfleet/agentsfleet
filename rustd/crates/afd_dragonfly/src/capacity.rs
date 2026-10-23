//! Measure retained and pending outbound stream state separately from cluster topology.

use crate::client::Dragonfly;
use crate::error::Result;
use crate::outbound::{OUTBOUND_CONSUMER_GROUP, OUTBOUND_STREAM_KEY};
use crate::streams::retain;
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
    pub async fn sample(redis: &Dragonfly, walk_cap: usize) -> Result<Self> {
        let keys = redis.scan_keys(OUTBOUND_STREAM_KEY, SCAN_PAGE).await?;
        let mut sample = Self {
            streams: count(keys.len()),
            ..Self::default()
        };
        for key in keys.iter().take(walk_cap) {
            let key = &redis.physical_key(key);
            sample.streams_walked += 1;
            sample.retained_entries += retain::length_of(redis, key).await?;
            if let Some(backlog) = retain::backlog_of(redis, key, OUTBOUND_CONSUMER_GROUP).await? {
                sample.pending_entries += backlog.pending;
            }
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
