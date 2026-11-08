//! How many fleets exist, by lifecycle status, as the last census pass counted
//! them.
//!
//! One snapshot cell per status, filled by a sweeper on its own cadence and
//! loaded by the gauge callback — the rule [`Observed`] states, five times
//! over. A status the grouped count did not return is published as ZERO,
//! because a count that succeeded and omitted a status has measured none of
//! them; a count that failed withdraws every cell, because then nothing was
//! measured at all. The two are different facts and the graph shows them
//! differently: a line at zero, and a gap.

use opentelemetry::KeyValue;

use crate::metrics::instrument::Reading;
use crate::metrics::label::fleet::FleetStatusLabel;
use crate::metrics::observed::Observed;
use crate::semconv;

#[cfg(test)]
mod tests;

/// The cells, one per member of [`FleetStatusLabel::ALL`] in that order.
///
/// A type rather than a bare static so a test can hold one of its own: the
/// process-wide instance below is what production publishes into, and a test
/// publishing into that would race every other test in its binary.
#[derive(Debug, Default)]
pub struct FleetCensus {
    cells: [Observed; FleetStatusLabel::ALL.len()],
}

impl FleetCensus {
    /// Cells nothing has published into yet, which observe nothing.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            cells: [const { Observed::new() }; FleetStatusLabel::ALL.len()],
        }
    }

    /// Publishes one pass's counts, every status at once.
    ///
    /// A status absent from `counts` is published as zero — see the module
    /// note. A status listed twice is summed, which cannot happen from a
    /// grouped count and costs nothing to be right about.
    pub fn publish(&self, counts: &[(FleetStatusLabel, u64)]) {
        for (cell, status) in self.cells.iter().zip(FleetStatusLabel::ALL) {
            let count: u64 = counts
                .iter()
                .filter(|(counted, _)| counted == status)
                .map(|(_, fleets)| *fleets)
                .sum();
            cell.publish(count);
        }
    }

    /// Withdraws every cell, so the gauge publishes nothing until the next
    /// successful pass.
    pub fn withdraw(&self) {
        for cell in &self.cells {
            cell.withdraw();
        }
    }

    /// One labelled reading per cell that holds a measurement.
    ///
    /// Atomics only — this runs under the SDK's pipeline lock.
    #[must_use]
    pub fn readings(&self) -> Vec<Reading> {
        self.cells
            .iter()
            .zip(FleetStatusLabel::ALL)
            .filter_map(|(cell, status)| {
                cell.load().map(|value| Reading {
                    attributes: vec![KeyValue::new(semconv::LABEL_STATUS, status.as_str())],
                    value,
                })
            })
            .collect()
    }
}

/// The cells production publishes into.
static FLEET_CENSUS: FleetCensus = FleetCensus::new();

/// Publishes what a completed census pass counted.
pub fn fleet_census_observed(counts: &[(FleetStatusLabel, u64)]) {
    FLEET_CENSUS.publish(counts);
}

/// Withdraws the census after a pass whose count failed.
pub fn fleet_census_withdrawn() {
    FLEET_CENSUS.withdraw();
}

/// What the gauge reads: the process-wide cells, labelled.
///
/// Public so a suite driving the sweeper against a live table can read back
/// exactly what the gauge would publish.
#[must_use]
pub fn fleet_census_readings() -> Vec<Reading> {
    FLEET_CENSUS.readings()
}
