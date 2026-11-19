//! Confirmed run targets; refresh never overwrites a newer offer, renewal or report.

mod snapshot;

use af_core::{error_code, id::Uuid7};
use af_wire::protocol::RUNS_MAX;
use afd_db::Db;
use afd_fleet::run::{Offered, Reported, Target};
use jiff::Timestamp;
use parking_lot::RwLock;
use std::collections::hash_map::Entry;

use crate::{Result, error::ErrorKind};
use snapshot::{RunRecord, RunSnapshot};

/// A short write lock makes replacement atomic; no cache guard crosses an await.
/// The revision protects local updates made while a database read is in flight.
#[derive(Debug, Default)]
struct CachedRuns {
    revision: u64,
    records: std::collections::HashMap<Uuid7, RunRecord>,
}

impl CachedRuns {
    /// A recheck may load a run from its committed issue before the offer
    /// records it. The same identifier is the same durable run, so the offer
    /// merges: its report facts join the loaded row, and a confirmed cancel or
    /// later deadline stays. Only a new run counts against the host bound.
    ///
    /// The durable issuer granted this run under the host's bound, so a full
    /// cache holds a lapsed run the next refresh has not dropped yet; those
    /// go first. A cache full of live runs is still refused.
    fn insert(&mut self, record: RunRecord, now: Timestamp) -> Result<()> {
        if self.records.len() >= RUNS_MAX && !self.records.contains_key(&record.id) {
            self.records.retain(|_, held| held.expires_at > now);
        }
        let held = self.records.len();
        match self.records.entry(record.id.clone()) {
            Entry::Occupied(mut loaded) => {
                let loaded = loaded.get_mut();
                loaded.expires_at = loaded.expires_at.max(record.expires_at);
                loaded.cancelled |= record.cancelled;
                loaded.target = record.target.or_else(|| loaded.target.take());
                loaded.reported = record.reported.or_else(|| loaded.reported.take());
                loaded.draft = record.draft.or_else(|| loaded.draft.take());
            }
            Entry::Vacant(_) if held >= RUNS_MAX => {
                return Err(ErrorKind::Protocol.into());
            }
            Entry::Vacant(slot) => {
                slot.insert(record);
            }
        }
        self.revision = self.revision.wrapping_add(1);
        Ok(())
    }
}

/// One bounded snapshot uses a nonpoisoning lock, so guards need no unwrap or panic recovery.
/// Reads and mutations are short; database I/O and frame encoding always happen after releasing it.
#[derive(Debug)]
pub(crate) struct RunCache(RwLock<CachedRuns>);

impl Default for RunCache {
    fn default() -> Self {
        Self(RwLock::new(CachedRuns::default()))
    }
}

impl RunCache {
    /// Rebuild only from durable, unexpired ownership; unlisted runs are allowed to lapse.
    pub(crate) async fn rebuild(database: &Db, runner: &Uuid7) -> Result<Self> {
        let cache = Self::default();
        cache.refresh(database, runner).await?;
        Ok(cache)
    }

    /// Load and validate outside the cache lock, then apply one complete snapshot.
    /// The returned cancellations come from Postgres even if a concurrent write wins.
    pub(super) async fn refresh(&self, database: &Db, runner: &Uuid7) -> Result<Vec<Uuid7>> {
        let revision = self.0.read().revision;
        let snapshot = RunSnapshot::load(database, runner).await?;
        let cancelled = snapshot.cancellations();
        self.apply(snapshot, revision, &cancelled);
        Ok(cancelled)
    }

    /// A stale read may add a durable cancellation, but cannot resurrect a completed run.
    fn apply(&self, snapshot: RunSnapshot, revision: u64, cancelled: &[Uuid7]) {
        let mut current = self.0.write();
        if current.revision == revision {
            let mut records = snapshot.into_records();
            // A run's report facts never change after its offer; the
            // database row a rebuild reads does not carry them.
            for (id, record) in &mut records {
                if record.reported.is_none() {
                    record.reported = current
                        .records
                        .get(id)
                        .and_then(|kept| kept.reported.clone());
                }
            }
            current.records = records;
        } else {
            for id in cancelled {
                if let Some(record) = current.records.get_mut(id) {
                    record.cancelled = true;
                }
            }
        }
        current.revision = current.revision.wrapping_add(1);
    }

    /// The durable issuer enforces capacity; a run past the host bound is an explicit error.
    pub(super) fn insert(&self, offered: Offered, now: Timestamp) -> Result<()> {
        self.0.write().insert(RunRecord::from(offered), now)
    }

    /// Expired and cancelled targets never authorize publication, regardless of connection health.
    pub(super) fn target(&self, run: &Uuid7, now: Timestamp) -> Option<Target> {
        self.0
            .read()
            .records
            .get(run)
            .filter(|record| !record.cancelled && record.expires_at > now)
            .and_then(|record| record.target.clone())
    }

    /// The draft `run` runs a turn of, when it is a draft-owned run.
    pub(super) fn draft(&self, run: &Uuid7) -> Option<Uuid7> {
        self.0
            .read()
            .records
            .get(run)
            .and_then(|record| record.draft.clone())
    }

    /// Whether `run` is a live, uncancelled draft-owned run, so its live
    /// answer may be published.
    pub(super) fn live_draft(&self, run: &Uuid7, now: Timestamp) -> bool {
        self.0.read().records.get(run).is_some_and(|record| {
            record.draft.is_some() && !record.cancelled && record.expires_at > now
        })
    }

    /// What `run`'s report reads, when the offer left it here.
    pub(super) fn reported(&self, run: &Uuid7) -> Option<Reported> {
        self.0
            .read()
            .records
            .get(run)
            .and_then(|record| record.reported.clone())
    }

    /// Late acknowledgements cannot shorten a newer confirmed deadline.
    pub(super) fn renew(&self, run: &Uuid7, expires_at: Timestamp) {
        self.update(run, |record| {
            record.expires_at = record.expires_at.max(expires_at);
        });
    }

    /// Removing the record advances the revision so an older refresh cannot recreate it.
    pub(super) fn forget(&self, run: &Uuid7) {
        let mut state = self.0.write();
        state.records.remove(run);
        state.revision = state.revision.wrapping_add(1);
    }

    /// Cancellation is monotonic for this unique run identifier.
    pub(super) fn cancel(&self, run: &Uuid7) {
        self.update(run, |record| record.cancelled = true);
    }

    fn update(&self, run: &Uuid7, change: impl FnOnce(&mut RunRecord)) {
        let mut state = self.0.write();
        if let Some(record) = state.records.get_mut(run) {
            change(record);
            state.revision = state.revision.wrapping_add(1);
        }
    }

    /// Only the runner's opening list needs reconciliation; absent database runs are never replayed.
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
                    Some(record) if record.cancelled => error_code::RUN_CANCELLED,
                    Some(record) if record.expires_at > now => return None,
                    _ => error_code::RUN_LOST,
                };
                Some((id.clone(), code.as_str()))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests;
