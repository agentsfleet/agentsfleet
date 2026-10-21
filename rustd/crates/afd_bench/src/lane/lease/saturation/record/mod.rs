//! What a saturation window writes into its report, and the one line the
//! make target prints.

use core::time::Duration;

use super::invariants::{Census, DOUBLED_CHAT, DOUBLED_FLEET, OVERSUBSCRIBED_HOSTS};
use super::live::Reads;
use super::probe::Watched;
use super::requester::Tally;
use super::stage::{TENANTS, WORKERS};
use crate::error::Result;
use crate::lane::lease::Parameters;
use crate::profile::Parameter;
use crate::report::{Latency, Report, count, latency, per_second};

/// Parameter key: connections the pool may open.
const POOL_SIZE: &str = "pool_size";

/// Parameter key: chat tenants whose drafts compete.
const CHAT_TENANTS: &str = "chat_tenants";

/// Parameter key: workers each host declares.
const HOST_WORKERS: &str = "host_workers";

/// Measurement key: setup chat leases issued.
const CHAT_LEASES: &str = "chat_leases";

/// Measurement key: fleet leases issued.
const FLEET_LEASES: &str = "fleet_leases";

/// Measurement key: polls made, lease or not.
const POLLS: &str = "polls";

/// Measurement key: polls and renewals the path refused.
const FAILURES: &str = "failures";

/// Measurement key: chat renewals the path accepted.
const RENEWALS: &str = "renewals";

/// Measurement key: leases of either kind per second of the window.
const LEASES_PER_SECOND: &str = "leases_per_second";

/// Measurement key: the tail of a poll that issued a lease.
const LEASE_P95_MS: &str = "lease_p95_ms";

/// Measurement key: the tail of a poll that issued a setup chat lease.
const CHAT_LEASE_P95_MS: &str = "chat_lease_p95_ms";

/// Measurement key: the longest poll that issued a setup chat lease.
const CHAT_LEASE_MAX_MS: &str = "chat_lease_max_ms";

/// Measurement key: holds the slow writer took on one tenant's drafts.
const SLOW_HOLDS: &str = "slow_writer_holds";

/// Measurement key: the tail of one pooled acquire.
const POOL_WAIT_P95_MS: &str = "pool_wait_p95_ms";

/// Measurement key: the longest pooled acquire.
const POOL_WAIT_MAX_MS: &str = "pool_wait_max_ms";

/// Measurement key: probe acquires the pool refused inside its budget.
const POOL_REFUSED: &str = "pool_refused";

/// Measurement key: the tail of an open transaction's age.
const HOLD_P95_MS: &str = "hold_p95_ms";

/// Measurement key: the oldest open transaction sampled.
const HOLD_MAX_MS: &str = "hold_max_ms";

/// Measurement key: the tail of one live-set read.
const LIVE_SET_P95_MS: &str = "live_set_p95_ms";

/// Measurement key: live-set reads the path refused.
const LIVE_SET_REFUSED: &str = "live_set_refused";

/// Measurement key: one when the pool served every connection afterwards.
const POOL_RECOVERED: &str = "pool_recovered";

/// The keys the one-line summary prints, in order.
const SUMMARY_KEYS: [&str; 8] = [
    LEASES_PER_SECOND,
    LEASE_P95_MS,
    CHAT_LEASE_P95_MS,
    POOL_WAIT_P95_MS,
    HOLD_P95_MS,
    LIVE_SET_P95_MS,
    DOUBLED_CHAT,
    OVERSUBSCRIBED_HOSTS,
];

/// Record the run's parameters, before anything is measured.
pub(super) fn parameters(report: &mut Report, parameters: Parameters, pool_size: u32) {
    report.created = true;
    report.parameter(Parameter::Fleets.name(), parameters.fleets);
    report.parameter(Parameter::Runners.name(), parameters.runners);
    report.parameter(POOL_SIZE, u64::from(pool_size));
    report.parameter(CHAT_TENANTS, TENANTS);
    report.parameter(HOST_WORKERS, u64::from(WORKERS));
}

/// Everything one window observed, folded across its tasks.
#[derive(Debug)]
pub(super) struct Window {
    pub(super) tally: Tally,
    pub(super) reads: Reads,
    pub(super) pool: Watched,
    pub(super) length: Duration,
    pub(super) slow_holds: u64,
}

impl Window {
    /// A window holding only what the probe saw.
    ///
    /// # Errors
    ///
    /// When an empty latency distribution will not be created.
    pub(super) fn new(pool: Watched) -> Result<Self> {
        Ok(Self {
            tally: Tally::new()?,
            reads: Reads::new()?,
            pool,
            length: Duration::ZERO,
            slow_holds: 0,
        })
    }

    /// Write every measurement and the census into `report`.
    pub(super) fn record(&self, report: &mut Report, census: &Census) {
        let leased = self.tally.chat + self.tally.fleet;
        let mut put = |key: &str, value: f64| report.measurement(key, value);
        put(CHAT_LEASES, count(self.tally.chat));
        put(FLEET_LEASES, count(self.tally.fleet));
        put(POLLS, count(self.tally.polls));
        put(FAILURES, count(self.tally.failures));
        put(RENEWALS, count(self.tally.renewals));
        put(SLOW_HOLDS, count(self.slow_holds));
        put(
            LEASES_PER_SECOND,
            per_second(leased, self.length.as_secs_f64()),
        );
        put(POOL_REFUSED, count(self.pool.refused));
        put(LIVE_SET_REFUSED, count(self.reads.refused));
        put(DOUBLED_CHAT, count(census.doubled_chat));
        put(DOUBLED_FLEET, count(census.doubled_fleet));
        put(OVERSUBSCRIBED_HOSTS, count(census.oversubscribed));
        put(POOL_RECOVERED, count(u64::from(census.recovered)));
        for (p95, max, measured) in [
            (LEASE_P95_MS, None, &self.tally.lease_latency),
            (
                CHAT_LEASE_P95_MS,
                Some(CHAT_LEASE_MAX_MS),
                &self.tally.chat_latency,
            ),
            (POOL_WAIT_P95_MS, Some(POOL_WAIT_MAX_MS), &self.pool.wait),
            (HOLD_P95_MS, Some(HOLD_MAX_MS), &self.pool.hold),
            (LIVE_SET_P95_MS, None, &self.reads.latency),
        ] {
            tail(&mut put, p95, max, measured);
        }
    }
}

/// A distribution's p95 and, when asked, its maximum; nothing when it is
/// empty, so a tail nobody sampled is never written as zero.
fn tail(put: &mut impl FnMut(&str, f64), p95: &str, max: Option<&str>, measured: &Latency) {
    if measured.is_empty() {
        return;
    }
    put(p95, measured.quantile_ms(latency::P95));
    if let Some(max) = max {
        put(max, measured.max_ms());
    }
}

/// The run's decisive numbers as one `key=value` line, for the terminal.
///
/// A key the run never measured is left out rather than printed as zero.
#[must_use]
pub fn summary(report: &Report) -> String {
    SUMMARY_KEYS
        .iter()
        .filter_map(|key| {
            report
                .measurements
                .get(*key)
                .map(|value| format!("{key}={value}"))
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests;
