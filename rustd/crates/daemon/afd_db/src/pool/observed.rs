//! Occupancy reads the pool; an acquisition guard survives retries and cancellation.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use af_observability::metrics::label::database::{AcquireOutcome, PoolRole};

use super::Db;
use crate::config::DbRole;

/// A nonblocking snapshot of one open role pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoolSnapshot {
    /// The role owning the connections.
    pub role: DbRole,
    /// Connections currently established.
    pub open: u32,
    /// Established connections not checked out.
    pub idle: u32,
    /// Configured ceiling, rather than an inferred capacity.
    pub capacity: u32,
    /// Acquisition requests currently pending, including a stalled retry.
    pub waiting: u64,
}

impl Db {
    /// Reads atomics only; a closed pool supplies no live reading.
    #[must_use]
    pub fn snapshot(&self) -> Option<PoolSnapshot> {
        if self.pool.is_closed() {
            return None;
        }
        let open = self.pool.size();
        // Release and close can race these independent SQLx counters.
        let idle = u32::try_from(self.pool.num_idle())
            .unwrap_or(open)
            .min(open);
        Some(PoolSnapshot {
            role: self.role,
            open,
            idle,
            capacity: self.max_connections,
            waiting: self.waiting.load(Ordering::Relaxed),
        })
    }
}

pub(super) struct Acquisition {
    waiting: Arc<AtomicU64>,
    role: PoolRole,
    started: Instant,
    outcome: AcquireOutcome,
}

impl Acquisition {
    pub(super) fn begin(db: &Db) -> Self {
        db.waiting.fetch_add(1, Ordering::Relaxed);
        Self {
            waiting: Arc::clone(&db.waiting),
            started: Instant::now(),
            role: match db.role {
                DbRole::Default => PoolRole::Default,
                DbRole::Api => PoolRole::Api,
                DbRole::Migrator => PoolRole::Migrator,
            },
            outcome: AcquireOutcome::Cancelled,
        }
    }

    pub(super) fn answered(
        &mut self,
        result: &crate::Result<sqlx::pool::PoolConnection<sqlx::Postgres>>,
    ) {
        self.outcome = match result {
            Ok(_) => AcquireOutcome::Acquired,
            Err(error) if error.is_pool_capacity() => AcquireOutcome::Capacity,
            Err(error) if error.is_acquire_stalled() => AcquireOutcome::Stalled,
            Err(_) => AcquireOutcome::Unavailable,
        };
    }
}

impl Drop for Acquisition {
    fn drop(&mut self) {
        self.waiting.fetch_sub(1, Ordering::Relaxed);
        af_observability::producers::database::acquired(
            self.role,
            self.outcome,
            self.started.elapsed(),
        );
    }
}
