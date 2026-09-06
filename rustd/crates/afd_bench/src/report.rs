//! The file a lane leaves behind, and the only place a measurement is written.
//!
//! # A result exists only if the run finished
//!
//! [`Report::write`] renders to a temporary path and renames onto the result
//! path, and rename is atomic within a directory. A lane killed mid-run leaves
//! no file, and a reader therefore never has to wonder whether the numbers in
//! front of it describe a complete run. A partial result is the one failure
//! this crate refuses to produce, because it is the one a person would quote.
//!
//! # Measurements are a map, and that is deliberate
//!
//! Four lanes report four different things — round trips per lease, retry
//! occupancy, memory per fleet — and a struct per lane would mean a comparison
//! per lane. One open map with named keys keeps a single reader, a single
//! comparison, and a rubric that greps the same path in every file.
//!
//! # Nothing here becomes a metric
//!
//! These numbers are synthetic, and the daemon's own instruments carry no
//! attribute distinguishing a bench run from real traffic
//! (`afd_observability::producers::fleet::lease_polled` records with an empty
//! attribute set). Publishing a lane's p95 as a series would therefore be
//! indistinguishable from production latency on an operator's dashboard. A
//! file cannot be mistaken for one.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::fixture::{FixtureLedger, RunPrefix};
use crate::profile::Profile;

pub mod compare;
pub mod latency;

pub use latency::Latency;

/// Directory a lane writes its result into.
pub const RESULTS_DIRECTORY: &str = "bench/results";

/// Directory holding the committed baseline a result is compared against.
pub const BASELINES_DIRECTORY: &str = "bench/baselines";

/// Extension both a result and a baseline carry.
const RESULT_EXTENSION: &str = "json";

/// Suffix the in-progress render carries until the run succeeds.
const PENDING_SUFFIX: &str = ".pending";

/// Measurement key: operations the lane sustained per second.
pub const RATE_PER_SECOND: &str = "rate_per_second";

/// Measurement key: the tail latency a lane leads with.
pub const P95_MS: &str = "p95_ms";

/// Measurement key: the tail latency behind it.
pub const P99_MS: &str = "p99_ms";

/// Measurement key: the slowest single operation.
pub const MAX_MS: &str = "max_ms";

/// Which question this file answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lane {
    /// What the system accepts, and where a steer costs.
    Steer,
    /// Leases per second, and the Postgres cost of each.
    Lease,
    /// What one delivery worker sustains, and what a slow destination costs.
    Outbound,
    /// What an idle fleet costs when there are a million of them.
    Cardinality,
}

impl Lane {
    /// The name this lane is written and asked for under.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Steer => "steer",
            Self::Lease => "lease",
            Self::Outbound => "outbound",
            Self::Cardinality => "cardinality",
        }
    }

    /// Where this lane's result for a profile is written.
    #[must_use]
    pub fn result_path(self, profile: Profile) -> PathBuf {
        Self::path_in(RESULTS_DIRECTORY, self, profile)
    }

    /// Where this lane's committed baseline for a profile lives.
    #[must_use]
    pub fn baseline_path(self, profile: Profile) -> PathBuf {
        Self::path_in(BASELINES_DIRECTORY, self, profile)
    }

    /// `<directory>/<lane>.<profile>.json`, the one naming rule both use.
    fn path_in(directory: &str, lane: Self, profile: Profile) -> PathBuf {
        Path::new(directory).join(format!("{}.{profile}.{RESULT_EXTENSION}", lane.name()))
    }
}

/// What a datastore was asked to do, and how long it spent doing it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct DatastoreCost {
    /// Commands issued to Redis, or round trips made to Postgres.
    pub operations: u64,
    /// Wall time spent waiting on them.
    pub time_ms: f64,
}

/// Where a run's cost landed, which is what says WHAT to fix.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Datastores {
    /// The Redis half.
    pub redis: DatastoreCost,
    /// The Postgres half.
    pub postgres: DatastoreCost,
}

/// What a deployed run created, and what its sweep removed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fixture {
    /// The prefix every created object carried.
    pub run_prefix: String,
    /// How many objects this run created.
    pub created: u64,
    /// How many the sweep removed, orphans from earlier runs included.
    pub swept: u64,
}

impl Fixture {
    /// The fixture block for a run that used this prefix and ledger.
    #[must_use]
    pub fn of(prefix: &RunPrefix, ledger: FixtureLedger) -> Self {
        Self {
            run_prefix: prefix.as_str().to_owned(),
            created: ledger.created_count(),
            swept: ledger.swept_count(),
        }
    }
}

/// Why a run stopped before its window elapsed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Abort {
    /// What the lane observed.
    pub observed_error_rate: f64,
    /// The profile threshold it crossed.
    pub threshold: f64,
}

/// One lane's answer, as the file a person and a rubric both read.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Report {
    /// Which question this answers.
    pub lane: Lane,
    /// Which profile it ran under.
    pub profile: String,
    /// Whether the run created the population it measured, or observed one.
    pub created: bool,
    /// What the caller asked for.
    pub parameters: BTreeMap<String, u64>,
    /// What the lane measured.
    pub measurements: BTreeMap<String, f64>,
    /// Samples over the run, for anything whose shape over time is the answer.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub series: BTreeMap<String, Vec<f64>>,
    /// Where the cost landed.
    pub datastores: Datastores,
    /// What was created and what was swept.
    pub fixture: Fixture,
    /// Present only when the run stopped early.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub abort: Option<Abort>,
}

impl Report {
    /// An empty report for a lane and profile, before anything is measured.
    #[must_use]
    pub fn new(lane: Lane, profile: Profile) -> Self {
        Self {
            lane,
            profile: profile.to_string(),
            created: false,
            parameters: BTreeMap::new(),
            measurements: BTreeMap::new(),
            series: BTreeMap::new(),
            datastores: Datastores::default(),
            fixture: Fixture::default(),
            abort: None,
        }
    }

    /// Record a parameter the caller chose.
    pub fn parameter(&mut self, name: &str, value: u64) {
        self.parameters.insert(name.to_owned(), value);
    }

    /// Record a measurement the lane took.
    pub fn measurement(&mut self, name: &str, value: f64) {
        self.measurements.insert(name.to_owned(), value);
    }

    /// Record the rate and the whole tail of a distribution at once.
    ///
    /// Every lane reports these four, so spelling them once here is what stops
    /// one lane calling its tail `p95` and another `p95_millis`.
    pub fn latency(&mut self, elapsed_seconds: f64, latency: &Latency) {
        #[expect(
            clippy::cast_precision_loss,
            reason = "an operation count past f64's exact range is not a run that finished"
        )]
        let operations = latency.count() as f64;
        if elapsed_seconds > 0.0 {
            self.measurement(RATE_PER_SECOND, operations / elapsed_seconds);
        }
        self.measurement(P95_MS, latency.quantile_ms(latency::P95));
        self.measurement(P99_MS, latency.quantile_ms(latency::P99));
        self.measurement(MAX_MS, latency.max_ms());
    }

    /// Write this report to its lane-and-profile path, atomically.
    ///
    /// # Errors
    ///
    /// [`Error::ResultUnwritable`] when the directory cannot be created, the
    /// render fails, or the rename does not land.
    pub fn write(&self, path: &Path) -> Result<()> {
        let rendered = serde_json::to_string_pretty(self)
            .map_err(|source| Error::ResultUnrenderable { source })?;
        if let Some(directory) = path.parent() {
            fs::create_dir_all(directory).map_err(|source| Error::ResultUnwritable {
                path: directory.to_path_buf(),
                source,
            })?;
        }
        let pending = path.with_extension(format!("{RESULT_EXTENSION}{PENDING_SUFFIX}"));
        fs::write(&pending, rendered).map_err(|source| Error::ResultUnwritable {
            path: pending.clone(),
            source,
        })?;
        fs::rename(&pending, path).map_err(|source| Error::ResultUnwritable {
            path: path.to_path_buf(),
            source,
        })
    }

    /// Read a report a lane wrote earlier, or a committed baseline.
    ///
    /// # Errors
    ///
    /// [`Error::ResultUnreadable`] when the file will not open, and
    /// [`Error::ResultUnparseable`] when its contents are not a report.
    pub fn read(path: &Path) -> Result<Self> {
        let raw = fs::read_to_string(path).map_err(|source| Error::ResultUnreadable {
            path: path.to_path_buf(),
            source,
        })?;
        serde_json::from_str(&raw).map_err(|source| Error::ResultUnparseable {
            path: path.to_path_buf(),
            source,
        })
    }
}

#[cfg(test)]
mod tests;
