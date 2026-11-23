//! Which question a result file answers, and where each one lives.
//!
//! Split from the report at the file cap: the lane names and their paths are
//! one concern — the make targets, the comparison and the result files all
//! read them — and the report's shape is another.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{BASELINES_DIRECTORY, RESULT_EXTENSION, RESULTS_DIRECTORY};
use crate::error::{Error, ErrorKind, Result};
use crate::profile::Profile;

/// Which question this file answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lane {
    /// What the system accepts, and where a message or webhook costs.
    Intake,
    /// Runs per second, and the Postgres cost of each.
    Runs,
    /// The production poll with more requesters than connections: rate, pool
    /// wait and hold time, and the run invariants under that pressure.
    #[serde(rename = "runs-saturation")]
    RunsSaturation,
    /// Messages paced to busy fleets: how long a message waits for a run.
    #[serde(rename = "runs-burst")]
    RunsBurst,
    /// What one delivery worker sustains, and what a slow destination costs.
    Delivery,
    /// What an idle fleet costs when there are a million of them.
    Idle,
    /// What the live stream costs per frame and per stream.
    Stream,
}

impl Lane {
    /// Every lane, in the order the make targets list them.
    ///
    /// The one list a binary iterates and the one a usage line is built from,
    /// so adding a lane is the enum arm and nothing else.
    pub const ALL: [Self; 7] = [
        Self::Intake,
        Self::Runs,
        Self::RunsSaturation,
        Self::RunsBurst,
        Self::Delivery,
        Self::Idle,
        Self::Stream,
    ];

    /// The name this lane is written and asked for under.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Intake => "intake",
            Self::Runs => "runs",
            Self::RunsSaturation => "runs-saturation",
            Self::RunsBurst => "runs-burst",
            Self::Delivery => "delivery",
            Self::Idle => "idle",
            Self::Stream => "stream",
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

impl core::str::FromStr for Lane {
    type Err = Error;

    fn from_str(name: &str) -> Result<Self> {
        Self::ALL
            .into_iter()
            .find(|lane| lane.name() == name)
            .ok_or_else(|| ErrorKind::UnknownLane { usage: LANE_USAGE }.into())
    }
}

/// How the lane names are spelled, for the refusal an unknown one raises.
const LANE_USAGE: &str =
    "expected one of intake, runs, runs-saturation, runs-burst, delivery, idle, stream";
