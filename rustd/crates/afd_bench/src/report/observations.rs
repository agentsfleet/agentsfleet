//! Recording and independently replaying raw measurement observations.

use std::collections::BTreeMap;

use super::{
    Calculation, Latency, MAX_MS, P95_MS, P99_MS, RATE_PER_SECOND, Report, count, latency,
};
use crate::error::{Error, Result};

impl Report {
    /// Record an integer observation as a scalar measurement.
    pub fn count(&mut self, name: &str, value: u64) {
        self.calculated(name, count(value), Calculation::count(value));
    }

    /// Record a ratio and both raw operands.
    pub fn ratio(&mut self, name: &str, numerator: u64, denominator: u64) {
        self.calculated(
            name,
            Calculation::ratio_value(numerator, denominator),
            Calculation::ratio(numerator, denominator),
        );
    }

    /// Record a boolean observation as zero or one.
    pub fn flag(&mut self, name: &str, value: bool) {
        self.calculated(
            name,
            if value { 1.0 } else { 0.0 },
            Calculation::flag(value),
        );
    }

    /// Record a monotonic counter delta and its two readings.
    pub fn difference(&mut self, name: &str, before: u64, after: u64) {
        self.calculated(
            name,
            count(after.saturating_sub(before)),
            Calculation::Difference { before, after },
        );
    }

    /// Record one calculated series point.
    pub fn series_value(&mut self, name: &str, value: f64, calculation: Calculation) {
        self.series.entry(name.to_owned()).or_default().push(value);
        self.series_calculations
            .entry(name.to_owned())
            .or_default()
            .push(calculation);
    }

    /// Record the rate and the whole tail of a distribution at once.
    pub fn latency(&mut self, elapsed: core::time::Duration, latency: &Latency) {
        if !elapsed.is_zero() {
            self.calculated(
                RATE_PER_SECOND,
                Calculation::rate_value(latency.count(), elapsed),
                Calculation::rate(latency.count(), elapsed),
            );
        }
        if latency.is_empty() {
            return;
        }
        self.calculated(
            P95_MS,
            latency.quantile_ms(latency::P95),
            Calculation::quantile(latency, 950_000),
        );
        self.calculated(
            P99_MS,
            latency.quantile_ms(latency::P99),
            Calculation::quantile(latency, 990_000),
        );
        self.calculated(MAX_MS, latency.max_ms(), Calculation::maximum(latency));
    }

    /// Record a scalar result with the raw observation that derives it.
    pub fn calculated(&mut self, name: &str, value: f64, calculation: Calculation) {
        self.measurements.insert(name.to_owned(), value);
        self.calculations.insert(name.to_owned(), calculation);
    }

    /// Recompute every stored number from its raw observations.
    ///
    /// # Errors
    ///
    /// Refuses missing, extra, malformed, or contradictory calculations.
    pub fn verify_calculations(&self) -> Result<()> {
        verify_map(&self.calculations, &self.measurements)?;
        if self.series_calculations.len() != self.series.len() {
            return Err(invalid("measurement series and raw series differ in shape"));
        }
        for (name, calculations) in &self.series_calculations {
            let values = calculations
                .iter()
                .map(Calculation::evaluate)
                .collect::<Result<Vec<_>>>()?;
            match self.series.get(name) {
                Some(stored)
                    if stored.len() == values.len()
                        && stored
                            .iter()
                            .zip(&values)
                            .all(|(actual, expected)| equivalent(*actual, *expected)) => {}
                Some(_) => {
                    return Err(invalid(&format!(
                        "measurement series {name} does not match its raw observations"
                    )));
                }
                None => return Err(invalid(&format!("measurement series {name} is missing"))),
            }
        }
        Ok(())
    }
}

fn verify_map(
    calculations: &BTreeMap<String, Calculation>,
    measurements: &BTreeMap<String, f64>,
) -> Result<()> {
    if calculations.len() != measurements.len() {
        return Err(invalid(
            "scalar measurements and calculations differ in shape",
        ));
    }
    for (name, calculation) in calculations {
        let expected = calculation.evaluate()?;
        match measurements.get(name) {
            Some(stored) if equivalent(*stored, expected) => {}
            Some(stored) => {
                return Err(invalid(&format!(
                    "measurement {name} is {stored:?}, raw observations replay as {expected:?}"
                )));
            }
            None => return Err(invalid(&format!("measurement {name} is missing"))),
        }
    }
    Ok(())
}

fn equivalent(stored: f64, expected: f64) -> bool {
    if stored.to_bits() == expected.to_bits() {
        return true;
    }
    let scale = stored.abs().max(expected.abs());
    stored.is_finite()
        && expected.is_finite()
        && (stored - expected).abs() <= scale * f64::EPSILON * 4.0
}

fn invalid(detail: &str) -> Error {
    Error::EvidenceInvalid(detail.to_owned())
}
