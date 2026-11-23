//! Which rings are still waiting for a hub, and what each resolved ring cost.
//!
//! A ring resolves once: the first hub to take it fixes its latency, and a
//! later hub taking the same fleet finds nothing to resolve. A ring no hub
//! takes within [`MISSED_AFTER`] resolves as missed, and a late take of it is
//! the same nothing, so a slow broker shows as misses rather than as a tail.

use core::time::Duration;
use std::collections::{HashMap, VecDeque};

use af_core::id::Uuid7;
use tokio::time::Instant;

use crate::error::Result;
use crate::report::{Latency, Report, count, latency};

/// How long a ring may wait for its first hub before it counts as missed.
pub(super) const MISSED_AFTER: Duration = Duration::from_secs(1);

/// Rings in flight, and the distribution of the ones that resolved.
#[derive(Debug)]
pub(super) struct Ledger {
    /// When each unresolved ring was published.
    pending: HashMap<Uuid7, Instant>,
    /// Unresolved rings in publish order, so the oldest expires first. A ring
    /// taken since stays here until it reaches the front, and is skipped.
    order: VecDeque<Uuid7>,
    latency: Latency,
    missed: u64,
}

impl Ledger {
    /// An empty ledger.
    ///
    /// # Errors
    ///
    /// `LatencyUnavailable` when the histogram cannot be built.
    pub(super) fn new() -> Result<Self> {
        Ok(Self {
            pending: HashMap::new(),
            order: VecDeque::new(),
            latency: Latency::new()?,
            missed: 0,
        })
    }

    /// Note a ring published for `fleet` at `at`.
    pub(super) fn rung(&mut self, fleet: Uuid7, at: Instant) {
        self.order.push_back(fleet.clone());
        self.pending.insert(fleet, at);
    }

    /// A hub took `fleet` at `at`; only the first take of a pending ring counts.
    ///
    /// # Errors
    ///
    /// `LatencyUnrecordable` when the histogram will not hold the value.
    pub(super) fn took(&mut self, fleet: &Uuid7, at: Instant) -> Result<()> {
        let Some(rung_at) = self.pending.remove(fleet) else {
            return Ok(());
        };
        let waited = at.saturating_duration_since(rung_at);
        if waited >= MISSED_AFTER {
            self.missed += 1;
            return Ok(());
        }
        self.latency.record(waited)
    }

    /// Resolve as missed every ring whose deadline has passed by `now`.
    pub(super) fn expire(&mut self, now: Instant) {
        while let Some(fleet) = self.order.front() {
            let due = self
                .pending
                .get(fleet)
                .map(|rung_at| now.saturating_duration_since(*rung_at) >= MISSED_AFTER);
            match due {
                Some(false) => return,
                Some(true) => {
                    self.pending.remove(fleet);
                    self.missed += 1;
                }
                None => {}
            }
            self.order.pop_front();
        }
    }

    /// When the oldest unresolved ring will count as missed, if any is left.
    pub(super) fn next_expiry(&self) -> Option<Instant> {
        self.order
            .iter()
            .find_map(|fleet| self.pending.get(fleet))
            .map(|rung_at| *rung_at + MISSED_AFTER)
    }

    /// Rings published and not yet resolved.
    pub(super) fn outstanding(&self) -> usize {
        self.pending.len()
    }

    /// Write this rung's three numbers, keyed by its hub count.
    ///
    /// A rung where every ring was missed has no latency to report, and its
    /// quantiles are left out rather than written as a zero nobody measured.
    pub(super) fn record(&self, report: &mut Report, hubs: usize) {
        if !self.latency.is_empty() {
            report.measurement(
                &format!("{P50_PREFIX}{hubs}"),
                self.latency.quantile_ms(latency::P50),
            );
            report.measurement(
                &format!("{P99_PREFIX}{hubs}"),
                self.latency.quantile_ms(latency::P99),
            );
        }
        report.measurement(&format!("{MISSED_PREFIX}{hubs}"), count(self.missed));
    }
}

/// Measurement key, before the hub count: the median ring's latency.
pub(super) const P50_PREFIX: &str = "wake_p50_ms_hubs_";

/// Measurement key, before the hub count: the 99th-percentile ring's latency.
pub(super) const P99_PREFIX: &str = "wake_p99_ms_hubs_";

/// Measurement key, before the hub count: rings no hub took in time.
pub(super) const MISSED_PREFIX: &str = "wake_missed_hubs_";
