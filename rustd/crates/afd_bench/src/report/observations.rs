//! Recording and independently replaying raw measurement observations.

use std::collections::BTreeMap;

use super::{
    Calculation, Latency, MAX_MS, P95_MS, P99_MS, RATE_PER_SECOND, Report, count, latency,
    per_second, ratio,
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
            ratio(numerator, denominator),
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
                per_second(latency.count(), elapsed.as_secs_f64()),
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
        if evaluate_map(&self.calculations)? != self.measurements {
            return Err(invalid("scalar measurements do not match raw observations"));
        }
        let mut series = BTreeMap::new();
        for (name, calculations) in &self.series_calculations {
            let values = calculations
                .iter()
                .map(Calculation::evaluate)
                .collect::<Result<Vec<_>>>()?;
            series.insert(name.clone(), values);
        }
        if series != self.series {
            return Err(invalid("measurement series do not match raw observations"));
        }
        Ok(())
    }
}

fn evaluate_map(calculations: &BTreeMap<String, Calculation>) -> Result<BTreeMap<String, f64>> {
    calculations
        .iter()
        .map(|(name, calculation)| Ok((name.clone(), calculation.evaluate()?)))
        .collect()
}

fn invalid(detail: &str) -> Error {
    Error::EvidenceInvalid(detail.to_owned())
}
