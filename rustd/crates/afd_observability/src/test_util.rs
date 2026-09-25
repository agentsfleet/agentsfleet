//! Reading the process's own counters back, for a suite that must.
//!
//! # Why a capturing exporter rather than a manual reader
//!
//! `ManualReader` sits behind `experimental_metrics_custom_reader` in
//! `opentelemetry_sdk` 0.32, and features unify across a workspace: turning
//! it on for a suite would turn it on for the daemon's provider too. A push
//! exporter that keeps every sum it was handed is public API and about as
//! much code as the flag would have been comment. `afd_bench::instrument`
//! reached the same conclusion first, for three families; this is that
//! shape for all of them.
//!
//! # The install is process-wide, so the capture is too
//!
//! `producers::install` writes a `OnceLock`: the producers bind to whichever
//! provider installed FIRST. A capture built per test would read an empty
//! provider every time after the first, so the sink and the provider live in
//! a `OnceLock` here as well, every [`Capture::install`] hands back the same
//! pair, and a suite reads a DELTA between two reads rather than expecting a
//! reset. Nothing else in the same test binary may install a provider of its
//! own; a suite that needs both is two binaries.

#![expect(
    clippy::expect_used,
    reason = "a test seam fails loudly on an unmet precondition; the manifest's restriction set is for the daemon"
)]

use core::time::Duration;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, OnceLock};

use opentelemetry::metrics::MeterProvider as _;
use opentelemetry_sdk::error::{OTelSdkError, OTelSdkResult};
use opentelemetry_sdk::metrics::data::{AggregatedMetrics, MetricData, ResourceMetrics};
use opentelemetry_sdk::metrics::exporter::PushMetricExporter;
use opentelemetry_sdk::metrics::{PeriodicReader, SdkMeterProvider, Temporality};

use crate::metrics::instrument::Instruments;
use crate::metrics::registry::Registry;
use crate::producers::{self, GaugeSources};
use crate::semconv;

/// What a poisoned capture lock reports.
const POISONED: &str = "the captured sums lock was poisoned";

/// One series: a family and its label pairs, sorted so the spelling is one.
type Series = (String, Vec<(String, String)>);

/// The last export's `u64` sums and histogram counts, every series.
#[derive(Debug, Default)]
struct Captured {
    latest: Mutex<BTreeMap<Series, u64>>,
}

/// The exporter half, so the trait lands on a type this crate owns.
#[derive(Debug)]
struct CapturingExporter {
    sink: Arc<Captured>,
}

impl PushMetricExporter for CapturingExporter {
    fn export(&self, metrics: &ResourceMetrics) -> impl Future<Output = OTelSdkResult> + Send {
        core::future::ready(self.capture(metrics))
    }

    fn force_flush(&self) -> OTelSdkResult {
        Ok(())
    }

    fn shutdown_with_timeout(&self, _timeout: Duration) -> OTelSdkResult {
        Ok(())
    }

    /// Cumulative, so every export carries the running total; a suite
    /// subtracts two readings itself, where the window is known.
    fn temporality(&self) -> Temporality {
        Temporality::Cumulative
    }
}

impl CapturingExporter {
    /// Take every `u64` sum and `f64` histogram count out of one export batch.
    fn capture(&self, metrics: &ResourceMetrics) -> OTelSdkResult {
        let mut captured = BTreeMap::new();
        for scope in metrics.scope_metrics() {
            for metric in scope.metrics() {
                let points: Vec<_> = match metric.data() {
                    AggregatedMetrics::U64(MetricData::Sum(sum)) => sum
                        .data_points()
                        .map(|point| (point.attributes().collect::<Vec<_>>(), point.value()))
                        .collect(),
                    AggregatedMetrics::F64(MetricData::Histogram(histogram)) => histogram
                        .data_points()
                        .map(|point| (point.attributes().collect::<Vec<_>>(), point.count()))
                        .collect(),
                    _ => continue,
                };
                for (attributes, value) in points {
                    let mut labels: Vec<(String, String)> = attributes
                        .into_iter()
                        .map(|attribute| {
                            (
                                attribute.key.as_str().to_owned(),
                                attribute.value.as_str().into_owned(),
                            )
                        })
                        .collect();
                    labels.sort();
                    captured.insert((metric.name().to_owned(), labels), value);
                }
            }
        }
        let mut latest = self
            .sink
            .latest
            .lock()
            .map_err(|_poisoned| OTelSdkError::InternalFailure(POISONED.to_owned()))?;
        *latest = captured;
        Ok(())
    }
}

/// The installed producer set, and the sink its exports land in.
#[derive(Debug, Clone)]
pub struct Capture {
    captured: Arc<Captured>,
    provider: SdkMeterProvider,
}

/// The one capture this process has, once anything has installed it.
static INSTALLED: OnceLock<Capture> = OnceLock::new();

/// Serialises the first build, so the provider kept is the one bound.
static BUILDING: Mutex<()> = Mutex::new(());

impl Capture {
    /// Install the process-wide producer set over a capturing reader.
    ///
    /// # Panics
    ///
    /// When the compiled-in census will not read or the instrument set will
    /// not build — both defects in this crate, not conditions a suite serves
    /// through.
    #[must_use]
    pub fn install() -> Self {
        if let Some(installed) = INSTALLED.get() {
            return installed.clone();
        }
        let _one_builder = BUILDING
            .lock()
            .expect("no builder panics while holding the lock");
        if let Some(installed) = INSTALLED.get() {
            return installed.clone();
        }
        let built = Self::build();
        let _ = INSTALLED.set(built);
        INSTALLED
            .get()
            .cloned()
            .expect("the capture was set a line above")
    }

    /// Build the provider and bind the producers to it.
    fn build() -> Self {
        let captured = Arc::new(Captured::default());
        let reader = PeriodicReader::builder(CapturingExporter {
            sink: Arc::clone(&captured),
        })
        .build();
        let provider = SdkMeterProvider::builder().with_reader(reader).build();
        let instruments = Instruments::new(
            Registry::declared().expect("the compiled-in census reads"),
            provider.meter(semconv::SCOPE_NAME),
            provider.meter(semconv::SCOPE_NAME),
        );
        // `false` means a sibling already installed, which is harmless: what
        // matters is that `installed()` answers `Some` afterwards.
        let _ = producers::install(&instruments, &GaugeSources::silent())
            .expect("every producer names a family the census declares");
        Self { captured, provider }
    }

    /// The running total of one counter series.
    ///
    /// `labels` is matched as a SET: order does not matter, but every pair
    /// must be present and no extra pair may be. A series nothing has
    /// recorded into reads zero.
    ///
    /// Flushes first: the periodic reader would otherwise answer with whatever
    /// the last interval happened to capture.
    ///
    /// # Panics
    ///
    /// When the provider will not flush or the capture lock's holder panicked.
    #[must_use]
    pub fn sum(&self, family: &str, labels: &[(&str, &str)]) -> u64 {
        self.provider
            .force_flush()
            .expect("an in-memory provider flushes");
        let mut wanted: Vec<(String, String)> = labels
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect();
        wanted.sort();
        self.captured
            .latest
            .lock()
            .expect(POISONED)
            .get(&(family.to_owned(), wanted))
            .copied()
            .unwrap_or_default()
    }

    /// The running observation count of one histogram series.
    #[must_use]
    pub fn histogram_count(&self, family: &str, labels: &[(&str, &str)]) -> u64 {
        self.sum(family, labels)
    }
}
