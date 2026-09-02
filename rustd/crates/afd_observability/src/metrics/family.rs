//! What a metric family IS, expressed as the traits it implements.
//!
//! # The shape, and where it comes from
//!
//! Prior art rather than invention: habitat's builder declares an enum per
//! domain and marks its kind with a trait
//! (`components/builder-api/src/server/services/metrics.rs` — `enum Counter`,
//! `impl CounterMetric for Counter`, `impl Metric for Counter`). Kind is
//! carried by WHICH TRAIT a type implements, never by a field somebody sets.
//!
//! The Zig daemon did the opposite, and it is worth naming why that is not
//! copied. `MetricMeta` carried `kind`, `monotonic`, `max_series`, `streamed`,
//! `cost`, `evented` and `live_read` as data — a configuration record for an
//! aggregator we hand-wrote because Zig has no OpenTelemetry SDK. Every one of
//! those fields selected a branch in code we owned. Here the SDK is the
//! aggregator, so the record describes a machine that no longer exists.
//!
//! # What the traits buy that a field cannot
//!
//! The census remains the authority on what kind each family is. These traits
//! are how that fact reaches the type system, and [`Registry::counter`],
//! [`Registry::histogram`] and [`Registry::gauge`] are where the two are held
//! against each other: the trait bound is checked by the compiler, the census
//! at construction, and a family whose type and contract disagree cannot be
//! bound to an instrument at all.
//!
//! [`Registry::counter`]: crate::metrics::registry::Registry::counter
//! [`Registry::histogram`]: crate::metrics::registry::Registry::histogram
//! [`Registry::gauge`]: crate::metrics::registry::Registry::gauge

#[cfg(test)]
mod tests;

/// Every metric family answers to the wire name the census declares for it.
///
/// `&'static str` and not `String`: a family name is a compile-time constant
/// the parity test grades byte-for-byte, and a name assembled at run time is
/// the beginning of an unbounded family set.
pub trait Metric {
    /// The wire name, byte-exact, as `docs/metrics.census.tsv` declares it.
    fn name(&self) -> &'static str;
}

/// A family that only goes up.
///
/// A marker: implementing it is what lets a family be bound as a counter, and
/// declining to implement it is what makes that a compile error everywhere
/// else. There is no `kind` field to set wrongly.
pub trait Counter: Metric {}

/// A family that distributes measurements over census-declared buckets.
pub trait Histogram: Metric {}

/// A family whose value is read at collection time from a published snapshot.
///
/// Distinct from the two above because it is never recorded: the daemon
/// publishes into an [`Observed`] cell on its own cadence and the SDK loads it
/// during collection. A gauge therefore has no recording method by
/// construction, which is what keeps work out of a callback that runs under
/// the SDK's pipeline lock.
///
/// [`Observed`]: crate::metrics::observed::Observed
pub trait Gauge: Metric {}

/// One declared family, with its kind carried in the type.
///
/// # Why a parameterised newtype rather than habitat's enum per domain
///
/// The principle is habitat's and is unchanged: kind is carried by WHICH TRAIT
/// a type implements, never by a field somebody sets. What differs is the
/// shape, and the reason is arithmetic. habitat declares a handful of metrics,
/// so an enum variant plus a `name()` match arm per family costs nothing. This
/// contract declares seventy-one, and the match arm would be a second
/// transcription of a name the census already holds — seventy-one chances for
/// a typo that compiles, in a file whose only job is to spell names correctly.
///
/// This carries the name once, in the declaration, and the kind stays in the
/// type: [`crate::metrics::registry::Registry::counter`] will not accept a
/// `Declared<GaugeKind>` and there is no `kind` field to set wrongly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Declared<K> {
    name: &'static str,
    kind: core::marker::PhantomData<K>,
}

impl<K> Declared<K> {
    /// The family the census declares under `name`.
    ///
    /// `const` so a family is a compile-time value: an assembled name is the
    /// beginning of an unbounded family set, and this makes that unwritable.
    #[must_use]
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            kind: core::marker::PhantomData,
        }
    }

    /// The wire name, in a constant context.
    ///
    /// [`Metric::name`] answers the same string and cannot be called from one:
    /// a trait method is not `const`. This exists so a table pairing families
    /// with the label sets they carry can itself be a `const`, which is what
    /// keeps that table from being a runtime list somebody forgets to extend.
    #[must_use]
    pub const fn wire_name(&self) -> &'static str {
        self.name
    }
}

impl<K> Metric for Declared<K> {
    fn name(&self) -> &'static str {
        self.name
    }
}

/// The kind marker for a family that only goes up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CounterKind;

/// The kind marker for a family read at collection time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GaugeKind;

/// The kind marker for a family distributed over declared buckets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HistogramKind;

impl Counter for Declared<CounterKind> {}

impl Gauge for Declared<GaugeKind> {}

impl Histogram for Declared<HistogramKind> {}
