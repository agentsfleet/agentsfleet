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
