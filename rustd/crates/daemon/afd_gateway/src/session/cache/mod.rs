//! Confirmed lease targets; refresh never overwrites a newer offer, renewal or report.

mod snapshot;

use af_core::{error_code, id::Uuid7};
use af_wire::protocol::LEASES_MAX;
use afd_db::Db;
use afd_fleet::lease::{Offered, Target};
use jiff::Timestamp;
use parking_lot::RwLock;

use crate::{Result, error::ErrorKind};
use snapshot::{LeaseRecord, LeaseSnapshot};

/// A short write lock makes replacement atomic; no cache guard crosses an await.
/// The revision protects local updates made while a database read is in flight.
#[derive(Debug, Default)]
struct CachedLeases {
    revision: u64,
    records: std::collections::HashMap<Uuid7, LeaseRecord>,
}

impl CachedLeases {
    fn insert(&mut self, record: LeaseRecord) -> Result<()> {
        if self.records.len() >= LEASES_MAX || self.records.contains_key(&record.id) {
            return Err(ErrorKind::Protocol.into());
        }
        self.records.insert(record.id.clone(), record);
        self.revision = self.revision.wrapping_add(1);
        Ok(())
    }
}

/// One bounded snapshot uses a nonpoisoning lock, so guards need no unwrap or panic recovery.
/// Reads and mutations are short; database I/O and frame encoding always happen after releasing it.
#[derive(Debug)]
pub(crate) struct LeaseCache(RwLock<CachedLeases>);

impl Default for LeaseCache {
    fn default() -> Self {
        Self(RwLock::new(CachedLeases::default()))
    }
}

impl LeaseCache {
    /// Rebuild only from durable, unexpired ownership; unlisted leases are allowed to lapse.
    pub(crate) async fn rebuild(database: &Db, runner: &Uuid7) -> Result<Self> {
        let cache = Self::default();
        cache.refresh(database, runner).await?;
        Ok(cache)
    }

    /// Load and validate outside the cache lock, then apply one complete snapshot.
    /// The returned cancellations come from Postgres even if a concurrent write wins.
    pub(super) async fn refresh(&self, database: &Db, runner: &Uuid7) -> Result<Vec<Uuid7>> {
        let revision = self.0.read().revision;
        let snapshot = LeaseSnapshot::load(database, runner).await?;
        let cancelled = snapshot.cancellations();
        self.apply(snapshot, revision, &cancelled);
        Ok(cancelled)
    }

    /// A stale read may add a durable cancellation, but cannot resurrect a completed lease.
    fn apply(&self, snapshot: LeaseSnapshot, revision: u64, cancelled: &[Uuid7]) {
        let mut current = self.0.write();
        if current.revision == revision {
            current.records = snapshot.into_records();
        } else {
            for id in cancelled {
                if let Some(record) = current.records.get_mut(id) {
                    record.cancelled = true;
                }
            }
        }
        current.revision = current.revision.wrapping_add(1);
    }

    /// The durable issuer enforces capacity; a conflicting cache entry is an explicit error.
    pub(super) fn insert(&self, offered: Offered) -> Result<()> {
        self.0.write().insert(LeaseRecord::from(offered))
    }

    /// Expired and cancelled targets never authorize publication, regardless of connection health.
    pub(super) fn target(&self, lease: &Uuid7, now: Timestamp) -> Option<Target> {
        self.0
            .read()
            .records
            .get(lease)
            .filter(|record| !record.cancelled && record.expires_at > now)
            .and_then(|record| record.target.clone())
    }

    /// Late acknowledgements cannot shorten a newer confirmed deadline.
    pub(super) fn renew(&self, lease: &Uuid7, expires_at: Timestamp) {
        self.update(lease, |record| {
            record.expires_at = record.expires_at.max(expires_at);
        });
    }

    /// Removing the record advances the revision so an older refresh cannot recreate it.
    pub(super) fn forget(&self, lease: &Uuid7) {
        let mut state = self.0.write();
        state.records.remove(lease);
        state.revision = state.revision.wrapping_add(1);
    }

    /// Cancellation is monotonic for this unique lease identifier.
    pub(super) fn cancel(&self, lease: &Uuid7) {
        self.update(lease, |record| record.cancelled = true);
    }

    fn update(&self, lease: &Uuid7, change: impl FnOnce(&mut LeaseRecord)) {
        let mut state = self.0.write();
        if let Some(record) = state.records.get_mut(lease) {
            change(record);
            state.revision = state.revision.wrapping_add(1);
        }
    }

    /// Only the runner's opening list needs reconciliation; absent database leases are never replayed.
    pub(super) fn reconciliation(
        &self,
        running: &[Uuid7],
        now: Timestamp,
    ) -> Vec<(Uuid7, &'static str)> {
        let state = self.0.read();
        running
            .iter()
            .filter_map(|id| {
                let code = match state.records.get(id) {
                    Some(record) if record.cancelled => error_code::RUN_LEASE_CANCELLED,
                    Some(record) if record.expires_at > now => return None,
                    _ => error_code::RUN_LEASE_LOST,
                };
                Some((id.clone(), code.as_str()))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests;
