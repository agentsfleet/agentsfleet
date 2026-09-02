//! The declared contract, as instruments a producer can actually record into.
//!
//! [`super::registry::Registry`] says what the families ARE; this builds them
//! and hands each one out exactly once. Between the two sits the check that
//! matters at boot: a producer names a family, the census is asked what that
//! family is, and a disagreement about kind or number type is refused here
//! rather than exported as a counter's total rendered as a distribution.
//!
//! # Claiming, and what it makes provable
//!
//! Every handout is recorded. That turns "does this family have a producer?"
//! from a question about the source tree — which only a human reading every
//! crate can answer, and only for as long as they remember — into a question a
//! booted process answers about itself: [`Instruments::unclaimed`] names every
//! family nobody asked for.
//!
//! It proves a producer EXISTS and that boot wired it, not that it ever fires.
//! What makes it a real check anyway is where the claim happens: a handle is
//! claimed where the producer is built, so a family whose producer was deleted
//! stops being claimed on the next boot.
//!
//! # Why the callbacks read a closure rather than the world
//!
//! An observable callback runs under the SDK's pipeline lock, with no
//! `catch_unwind` and no timeout. A callback that touches Redis or takes a
//! lock some other thread holds does not slow one metric down — it stalls the
//! whole pipeline, and the first symptom is every family going silent at once.
//! So a gauge is registered with a closure that only LOADS, and what it loads
//! from is [`super::observed::Observed`].

use std::collections::BTreeSet;
use std::sync::{Mutex, PoisonError};

use opentelemetry::KeyValue;
use opentelemetry::metrics::{Counter, Histogram, Meter, ObservableGauge};

use crate::error::{Error, Result};
use crate::metrics::family::{
    Counter as CounterFamily, Gauge as GaugeFamily, Histogram as HistogramFamily,
};
use crate::metrics::registry::{Family, Number, Registry, Temporality};

pub use self::view::series_ceilings;

mod view;

#[cfg(test)]
mod tests;

/// One value a gauge callback publishes, and what it is attributed to.
///
/// A gauge that is absent publishes NO reading rather than a zero — the rule
/// [`super::observed::Observed`] exists for. A gap in a graph is the truth; a
/// zero is a claim nobody measured.
#[derive(Debug, Clone)]
pub struct Reading {
    /// The label values this reading is attributed to; empty for a family that
    /// declares no labels.
    pub attributes: Vec<KeyValue>,
    /// What was read.
    pub value: u64,
}

impl Reading {
    /// A reading with no labels, for the families that declare none.
    #[must_use]
    pub const fn unlabelled(value: u64) -> Self {
        Self {
            attributes: Vec::new(),
            value,
        }
    }
}

/// Every instrument this process records through, built from the census.
///
/// Not `Clone`: there is one per process by construction, and a second would
/// be a second set of series under the same names.
#[derive(Debug)]
pub struct Instruments {
    /// The meter a family reporting a running total is built on.
    ///
    /// Gauges are built here too. They carry no window at all, so either
    /// provider would describe them identically, and putting them beside the
    /// cumulative families keeps the delta provider to exactly the families
    /// whose payload IS a window.
    cumulative: Meter,
    /// The meter a family reporting only its window is built on.
    ///
    /// A second provider and not a setting, because the SDK asks the EXPORTER
    /// which temporality it wants and aggregates to match — one provider would
    /// silently rewrite the cost families' temporality to whatever the runtime
    /// families use.
    delta: Meter,
    registry: Registry,
    claimed: Mutex<BTreeSet<&'static str>>,
    /// The observable handles, kept alive.
    ///
    /// An observable instrument's callback lives as long as its handle. Dropped
    /// on the floor at construction, every gauge in this daemon would go silent
    /// while every counter kept working — which is the hardest kind of
    /// telemetry defect to notice, because the dashboard still draws.
    observed: Mutex<Vec<ObservableGauge<u64>>>,
}

impl Instruments {
    /// Binds the declared contract to a meter.
    #[must_use]
    pub fn new(registry: Registry, cumulative: Meter, delta: Meter) -> Self {
        Self {
            cumulative,
            delta,
            registry,
            claimed: Mutex::new(BTreeSet::new()),
            observed: Mutex::new(Vec::new()),
        }
    }

    /// The contract these instruments were built from.
    #[must_use]
    pub const fn registry(&self) -> &Registry {
        &self.registry
    }

    /// A counter of whole things.
    ///
    /// # Errors
    ///
    /// [`Error::UnknownFamily`] when the census declares no such name,
    /// [`Error::KindMismatch`] when it declares it something other than a
    /// counter, and [`Error::NumberMismatch`] when it counts in `f64`.
    pub fn counter_u64<M: CounterFamily>(&self, family: &M) -> Result<Counter<u64>> {
        let declared = self.declared_counter(family, Number::U64)?;
        Ok(self
            .meter_for(declared)
            .u64_counter(declared.name.to_string())
            .with_unit(declared.unit.to_string())
            .with_description(declared.watch_for.to_string())
            .build())
    }

    /// A counter of quantities that do not divide evenly.
    ///
    /// # Errors
    ///
    /// As [`Instruments::counter_u64`], with the number check inverted.
    pub fn counter_f64<M: CounterFamily>(&self, family: &M) -> Result<Counter<f64>> {
        let declared = self.declared_counter(family, Number::F64)?;
        Ok(self
            .meter_for(declared)
            .f64_counter(declared.name.to_string())
            .with_unit(declared.unit.to_string())
            .with_description(declared.watch_for.to_string())
            .build())
    }

    /// A distribution over the bucket bounds the census declares for it.
    ///
    /// The bounds come from the contract, never from the call site: bounds
    /// chosen where a measurement is taken are bounds that differ between two
    /// emitters of the same family, which is a histogram nobody can compare.
    ///
    /// # Errors
    ///
    /// [`Error::UnknownFamily`], [`Error::KindMismatch`] when the census
    /// declares it something other than a histogram, or
    /// [`Error::NumberMismatch`].
    pub fn histogram_f64<M: HistogramFamily>(&self, family: &M) -> Result<Histogram<f64>> {
        let declared = self.registry.histogram(family)?;
        check_number(declared, Number::F64)?;
        self.claim(family.name());
        Ok(self
            .meter_for(declared)
            .f64_histogram(declared.name.to_string())
            .with_unit(declared.unit.to_string())
            .with_description(declared.watch_for.to_string())
            .with_boundaries(declared.bounds.clone())
            .build())
    }

    /// Registers a gauge the SDK reads at collection time.
    ///
    /// `read` runs under the pipeline lock, so it must only load — see the
    /// module note. An empty answer publishes nothing at all, which is how a
    /// publisher that has not read yet, or whose last read failed, leaves a
    /// gap rather than a measurement nobody took.
    ///
    /// # Errors
    ///
    /// [`Error::UnknownFamily`], or [`Error::KindMismatch`] when the census
    /// declares it something other than a gauge.
    pub fn gauge_u64<M, F>(&self, family: &M, read: F) -> Result<()>
    where
        M: GaugeFamily,
        F: Fn() -> Vec<Reading> + Send + Sync + 'static,
    {
        let declared = self.registry.gauge(family)?;
        self.claim(family.name());
        let gauge = self
            .meter_for(declared)
            .u64_observable_gauge(declared.name.to_string())
            .with_unit(declared.unit.to_string())
            .with_description(declared.watch_for.to_string())
            .with_callback(move |observer| {
                for reading in read() {
                    observer.observe(reading.value, &reading.attributes);
                }
            })
            .build();
        self.observed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(gauge);
        Ok(())
    }

    /// Every declared family nobody has claimed an instrument for.
    ///
    /// In wire-name order, because a drift message that names families in the
    /// same sequence on every run is one a reader can diff against the census.
    #[must_use]
    pub fn unclaimed(&self) -> Vec<Box<str>> {
        let claimed = self.claimed.lock().unwrap_or_else(PoisonError::into_inner);
        self.registry
            .families()
            .filter(|family| !claimed.contains(&*family.name))
            .map(|family| family.name.clone())
            .collect()
    }

    /// The meter a family's declared temporality routes it to.
    ///
    /// The census column decides, not the call site: a family built on the
    /// wrong provider exports under a temporality it never declared, and every
    /// downstream `rate()` over it is then wrong in a way no error reports.
    const fn meter_for(&self, declared: &Family) -> &Meter {
        match declared.temporality {
            Some(Temporality::Delta) => &self.delta,
            Some(Temporality::Cumulative) | None => &self.cumulative,
        }
    }

    /// The census row for a counter, checked for kind and number.
    fn declared_counter<M: CounterFamily>(&self, family: &M, number: Number) -> Result<&Family> {
        let declared = self.registry.counter(family)?;
        check_number(declared, number)?;
        self.claim(family.name());
        Ok(declared)
    }

    /// Records that somebody took an instrument for `family`.
    fn claim(&self, family: &'static str) {
        self.claimed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(family);
    }
}

/// The census and the caller agree about what a family counts IN.
///
/// Separate from the kind check because the two failures are different edits:
/// a kind mismatch means the trait and the contract disagree about what the
/// family IS, and this means they agree about that and disagree about the
/// number — which exports whole counts as a floating-point series, or truncates
/// a fractional one to nothing.
fn check_number(declared: &Family, claimed: Number) -> Result<()> {
    if declared.number == claimed {
        return Ok(());
    }
    Err(Error::NumberMismatch {
        family: declared.name.clone(),
        declared: declared.number.spelling(),
        claimed: claimed.spelling(),
    })
}
