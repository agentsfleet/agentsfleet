//! Which hash a fleet's readiness mark lives in, and the cursor a poll turns.
//!
//! # Why the index is several hashes
//!
//! One hash is one slot on one node, and a poll samples it at random. Under a
//! skewed population — a few thousand fleets ready in one workspace, three in
//! another — the sample is dominated by whichever fleets are many, and the
//! three are found by luck. Spread over a fixed set of hashes, each hash is
//! its own slot, a hot workspace fills only the hashes its fleets land in,
//! and a poll that visits every hash in turn finds every fleet inside one
//! rotation whatever the skew. The measurement that chose the count is
//! recorded in `docs/architecture/datastore_scaling.md`.
//!
//! # Why the hash is the cluster's own
//!
//! A fleet's partition has to be the same number wherever it is computed —
//! the ingress that marks, the poll that clears, the sweeper that re-marks —
//! and for the life of every mark already written. `crc16` over the fleet id
//! is the checksum the cluster itself keys slots by: deterministic, stable
//! across builds, and already compiled into this tree by the driver.

use std::sync::Arc;
use std::sync::atomic::{AtomicU16, Ordering};

/// How many hashes the readiness index is spread over.
///
/// Divides `u16::MAX + 1`, so a cursor that wraps its counter continues the
/// rotation instead of skipping a partition at the seam; the assertion below
/// holds the count to that.
pub const READY_PARTITIONS: u16 = 16;

const _: () = assert!(
    (u16::MAX as u32 + 1).is_multiple_of(READY_PARTITIONS as u32),
    "the partition count must divide the cursor's range"
);

/// The key every partition hangs off; the partition's number follows it as
/// a hash tag, so the number alone decides the slot.
pub const READY_INDEX_KEY: &str = "fleet:ready";

/// One of the readiness index's hashes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Partition(u16);

impl Partition {
    /// The partition numbered `index`, or `None` past the last.
    #[must_use]
    pub const fn new(index: u16) -> Option<Self> {
        if index < READY_PARTITIONS {
            Some(Self(index))
        } else {
            None
        }
    }

    /// The partition a fleet's mark lives in.
    #[must_use]
    pub fn of(fleet_id: &str) -> Self {
        let checksum = crc16::State::<crc16::XMODEM>::calculate(fleet_id.as_bytes());
        Self(checksum % READY_PARTITIONS)
    }

    /// Every partition, in order.
    pub fn all() -> impl Iterator<Item = Self> {
        (0..READY_PARTITIONS).map(Self)
    }

    /// The partition's number.
    #[must_use]
    pub const fn index(self) -> u16 {
        self.0
    }

    /// The key this partition's hash lives under.
    #[must_use]
    pub fn key(self) -> String {
        format!("{READY_INDEX_KEY}:{{{}}}", self.0)
    }
}

/// Where the next poll looks.
///
/// One counter shared by every clone, so every poll made through a process
/// advances the same rotation: the partitions are visited in turn whichever
/// runner asks, and a partition is never starved because the runners that
/// happened to poll all started at the same place. Cheap to clone and held
/// by the lease store, which is `Clone` for the same reason.
#[derive(Debug, Clone, Default)]
pub struct ReadyCursor {
    next: Arc<AtomicU16>,
}

impl ReadyCursor {
    /// A cursor at the first partition.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A cursor whose next partition is derived from `position`.
    ///
    /// The seam is what this exists to prove: a position at the top of the
    /// counter's range must continue into the first partition, not skip one.
    #[cfg(test)]
    pub(crate) fn starting_at(position: u16) -> Self {
        Self {
            next: Arc::new(AtomicU16::new(position)),
        }
    }

    /// The partition this poll reads, moving the cursor past it.
    ///
    /// `Relaxed` is enough: the only property is that consecutive calls hand
    /// out consecutive partitions, and a single atomic counter gives that
    /// under any ordering.
    #[must_use]
    pub fn advance(&self) -> Partition {
        Partition(self.next.fetch_add(1, Ordering::Relaxed) % READY_PARTITIONS)
    }
}

#[cfg(test)]
mod tests;
