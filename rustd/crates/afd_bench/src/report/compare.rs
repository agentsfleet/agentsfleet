//! The delta between a result and its baseline, and why it never fails.
//!
//! # No throughput threshold, ever
//!
//! These lanes run on shared Continuous Integration (CI) runners and, on the
//! deployed profiles, against environments carrying other work. A gate on a
//! rate would fire on a noisy neighbour as readily as on a regression, and a
//! gate that cries wolf gets muted — at which point the real regression sails
//! through the muted gate. So the comparison prints a delta with its direction
//! and exits zero whichever way it moved. A person decides what a number
//! means; this only makes sure the number is in front of them.
//!
//! The two things it DOES refuse are absences of a measurement rather than
//! measurements: a result that will not parse, and nothing else.

use core::fmt::Write as _;
use std::path::Path;

use crate::error::Result;
use crate::report::Report;

/// Printed when a lane and profile have no committed baseline yet.
///
/// Its own outcome rather than a zero delta: a first run has nothing to
/// compare against, and inventing a baseline out of the result would make
/// every first run look like a perfect match forever after.
pub const NO_BASELINE: &str = "no baseline recorded yet";

/// Printed where a value or a percentage does not exist.
const ABSENT: &str = "—";

/// Below this, a delta is noise on a shared runner rather than a change.
const NOTEWORTHY_FRACTION: f64 = 0.05;

/// One measurement, as it moved.
#[derive(Debug, Clone, PartialEq)]
pub struct Delta {
    /// The measurement key.
    pub name: String,
    /// What the baseline recorded.
    pub baseline: f64,
    /// What this run measured.
    pub current: f64,
}

impl Delta {
    /// How far it moved, as a fraction of the baseline.
    ///
    /// A baseline of zero has no fraction to report — dividing by it would
    /// produce an infinity a reader would take for a real number — so a change
    /// away from zero reports as [`f64::NAN`] and prints without a percentage.
    #[must_use]
    pub fn fraction(&self) -> f64 {
        if self.baseline == 0.0 {
            return f64::NAN;
        }
        (self.current - self.baseline) / self.baseline
    }

    /// Whether this moved far enough to be worth a reader's attention.
    #[must_use]
    pub fn is_noteworthy(&self) -> bool {
        let fraction = self.fraction();
        fraction.is_nan() || fraction.abs() >= NOTEWORTHY_FRACTION
    }
}

/// Every measurement the two files share, plus what only one of them has.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Comparison {
    /// Parameters that differ, which is the first thing a reader must know:
    /// a delta between runs of different populations is not a regression.
    pub parameters: Vec<ParameterChange>,
    /// Measurements present in both, with how they moved.
    pub deltas: Vec<Delta>,
    /// Measurements this run reports that the baseline does not.
    pub added: Vec<String>,
    /// Measurements the baseline carries that this run did not report.
    pub missing: Vec<String>,
}

/// One parameter that is not the same in both files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParameterChange {
    /// The parameter's name.
    pub name: String,
    /// What the baseline ran with, if it recorded one.
    pub baseline: Option<u64>,
    /// What this run ran with, if it recorded one.
    pub current: Option<u64>,
}

impl Comparison {
    /// Compare a result against a baseline.
    #[must_use]
    pub fn of(current: &Report, baseline: &Report) -> Self {
        let mut comparison = Self::default();
        let names: std::collections::BTreeSet<&String> = current
            .parameters
            .keys()
            .chain(baseline.parameters.keys())
            .collect();
        for name in names {
            let (before, after) = (baseline.parameters.get(name), current.parameters.get(name));
            if before != after {
                comparison.parameters.push(ParameterChange {
                    name: name.clone(),
                    baseline: before.copied(),
                    current: after.copied(),
                });
            }
        }
        for (name, value) in &current.measurements {
            match baseline.measurements.get(name) {
                Some(before) => comparison.deltas.push(Delta {
                    name: name.clone(),
                    baseline: *before,
                    current: *value,
                }),
                None => comparison.added.push(name.clone()),
            }
        }
        for name in baseline.measurements.keys() {
            if !current.measurements.contains_key(name) {
                comparison.missing.push(name.clone());
            }
        }
        comparison
    }

    /// The comparison as the lines a person reads.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = String::new();
        for change in &self.parameters {
            let _ = writeln!(
                out,
                "! {:<24} {:>12} -> {:>12}  (different parameters: the deltas below are not a regression)",
                change.name,
                change.baseline.map_or(ABSENT.to_owned(), |v| v.to_string()),
                change.current.map_or(ABSENT.to_owned(), |v| v.to_string()),
            );
        }
        for delta in &self.deltas {
            let fraction = delta.fraction();
            let movement = if fraction.is_nan() {
                format!("{ABSENT:>7}")
            } else {
                format!("{:+7.1}%", fraction * 100.0)
            };
            let flag = if delta.is_noteworthy() { "*" } else { " " };
            let _ = writeln!(
                out,
                "{flag} {:<24} {:>12.3} -> {:>12.3}  {movement}",
                delta.name, delta.baseline, delta.current
            );
        }
        for name in &self.added {
            let _ = writeln!(out, "+ {name:<24} (not in the baseline)");
        }
        for name in &self.missing {
            let _ = writeln!(out, "- {name:<24} (this run did not report it)");
        }
        out
    }
}

/// Compare a result against its baseline, rendering whichever outcome applies.
///
/// An absent baseline is an outcome, not a failure. An unreadable RESULT is a
/// failure, because it is the file the run was supposed to produce.
///
/// # Errors
///
/// Whatever [`Report::read`] raises for the result path.
pub fn against_baseline(result_path: &Path, baseline_path: &Path) -> Result<String> {
    let current = Report::read(result_path)?;
    if !baseline_path.exists() {
        return Ok(format!("{NO_BASELINE}: {}\n", baseline_path.display()));
    }
    let baseline = Report::read(baseline_path)?;
    Ok(Comparison::of(&current, &baseline).render())
}

#[cfg(test)]
mod tests;
