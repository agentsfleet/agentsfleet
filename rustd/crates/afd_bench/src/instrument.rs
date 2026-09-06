//! Reading the daemon's own lease counters back, in the bench process.
//!
//! # Why not just count the queries
//!
//! Wrapping `sqlx` and tallying statements would measure the wrapper: it would
//! count what the bench issued, not what the lease path issued, and it would
//! keep counting correctly after somebody changed the lease path underneath
//! it. `afd_fleet`'s `select` already tallies its own round trips and publishes
//! them through `producers::fleet::lease_polled`, so the honest number is the
//! one the daemon would report about itself. This module installs a provider
//! that exports nowhere and asks its reader for the sums.
//!
//! # Why a capturing exporter rather than a manual reader
//!
//! `ManualReader` — the obvious way to ask a provider for its numbers — sits
//! behind `experimental_metrics_custom_reader` in `opentelemetry_sdk` 0.32.1,
//! and features unify across a workspace: turning it on here would turn it on
//! for the daemon's own provider too. A push exporter that keeps the three
//! sums it was handed is public API, local to this crate, and about as much
//! code as the feature flag would have been comment.
//!
//! # The install is process-wide and happens once
//!
//! `producers::install` writes a `OnceLock`, so a second call in the same
//! process is a no-op that returns `false`. That is why a lane installs at
//! startup rather than per run, and why the readback is expressed as a DELTA
//! between two reads instead of a reset.

use core::time::Duration;
use std::sync::{Arc, Mutex};

use afd_observability::metrics::instrument::Instruments;
use afd_observability::metrics::registry::Registry;
use afd_observability::producers::{self, GaugeSources};
use afd_observability::semconv;
use opentelemetry::metrics::MeterProvider as _;
use opentelemetry_sdk::error::{OTelSdkError, OTelSdkResult};
use opentelemetry_sdk::metrics::data::{
    AggregatedMetrics, MetricData, ResourceMetrics, SumDataPoint,
};
use opentelemetry_sdk::metrics::exporter::PushMetricExporter;
use opentelemetry_sdk::metrics::{PeriodicReader, SdkMeterProvider, Temporality};

use crate::error::{Error, Result};

/// Total polls the lease path has made.
const POLLS: &str = "agentsfleet_lease_polls_total";

/// Total fleets those polls examined.
const CANDIDATES: &str = "agentsfleet_lease_poll_candidates_scanned_total";

/// Total Postgres round trips those polls issued.
const ROUNDTRIPS: &str = "agentsfleet_lease_poll_db_roundtrips_total";

/// What a poisoned capture lock reports, rather than panicking a bench run.
const POISONED: &str = "the captured counters lock was poisoned";

/// What the lease path has cost since the process started.
///
/// Cumulative, because the underlying instruments are counters. A lane reads
/// once before its window and once after, and reports the difference.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PollCounters {
    /// Polls made.
    pub polls: u64,
    /// Fleets those polls examined.
    pub candidates: u64,
    /// Postgres round trips those polls issued.
    pub roundtrips: u64,
}

impl PollCounters {
    /// What happened between an earlier reading and this one.
    #[must_use]
    pub const fn since(self, earlier: Self) -> Self {
        Self {
            polls: self.polls.saturating_sub(earlier.polls),
            candidates: self.candidates.saturating_sub(earlier.candidates),
            roundtrips: self.roundtrips.saturating_sub(earlier.roundtrips),
        }
    }

    /// Postgres round trips per poll, or zero when nothing polled.
    #[must_use]
    pub fn roundtrips_per_poll(self) -> f64 {
        if self.polls == 0 {
            return 0.0;
        }
        #[expect(
            clippy::cast_precision_loss,
            reason = "a poll count past f64's exact range is not a run that finished"
        )]
        {
            self.roundtrips as f64 / self.polls as f64
        }
    }
}

/// Keeps the last export's lease counters, and nothing else.
///
/// Every other family the daemon declares passes through untouched: this is a
/// sink, not a store, and the three names below are the only ones it reads.
#[derive(Debug, Default)]
struct CapturedCounters {
    latest: Mutex<PollCounters>,
}

/// The exporter half, so the trait lands on a type this crate owns.
#[derive(Debug)]
struct CapturingExporter {
    sink: Arc<CapturedCounters>,
}

impl PushMetricExporter for CapturingExporter {
    /// Sync body behind the trait's future: capturing three sums out of a
    /// borrowed batch awaits nothing, and an `async fn` that never yields
    /// would only be a promise this exporter does not keep.
    fn export(&self, metrics: &ResourceMetrics) -> impl Future<Output = OTelSdkResult> + Send {
        core::future::ready(self.capture(metrics))
    }

    fn force_flush(&self) -> OTelSdkResult {
        Ok(())
    }

    fn shutdown_with_timeout(&self, _timeout: Duration) -> OTelSdkResult {
        Ok(())
    }

    /// Cumulative, so every export carries the running total rather than the
    /// delta since the last one. A lane subtracts two readings itself, which
    /// keeps the arithmetic where the window is known.
    fn temporality(&self) -> Temporality {
        Temporality::Cumulative
    }
}

impl CapturingExporter {
    /// Take the three lease sums out of one export batch.
    fn capture(&self, metrics: &ResourceMetrics) -> OTelSdkResult {
        let mut captured = PollCounters::default();
        for scope in metrics.scope_metrics() {
            for metric in scope.metrics() {
                let AggregatedMetrics::U64(MetricData::Sum(sum)) = metric.data() else {
                    continue;
                };
                let total: u64 = sum.data_points().map(SumDataPoint::value).sum();
                match metric.name() {
                    POLLS => captured.polls = total,
                    CANDIDATES => captured.candidates = total,
                    ROUNDTRIPS => captured.roundtrips = total,
                    _ => {}
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

/// The installed instrument set, and the sink its exports land in.
#[derive(Debug)]
pub struct LeaseInstrument {
    captured: Arc<CapturedCounters>,
    provider: SdkMeterProvider,
}

impl LeaseInstrument {
    /// Install the process-wide producer set over an in-memory reader.
    ///
    /// # Errors
    ///
    /// [`Error::InstrumentUnavailable`] when the compiled-in census will not
    /// read or the instrument set will not build.
    pub fn install() -> Result<Self> {
        let captured = Arc::new(CapturedCounters::default());
        let reader = PeriodicReader::builder(CapturingExporter {
            sink: Arc::clone(&captured),
        })
        .build();
        let provider = SdkMeterProvider::builder().with_reader(reader).build();
        let registry = Registry::declared()?;
        let instruments = Instruments::new(
            registry,
            provider.meter(semconv::SCOPE_NAME),
            provider.meter(semconv::SCOPE_NAME),
        );
        // `false` means a sibling already installed, which is the ordinary case
        // in a test binary and harmless: what matters is that `installed()`
        // answers `Some` afterwards, not which call put it there.
        producers::install(&instruments, &GaugeSources::silent())?;
        Ok(Self { captured, provider })
    }

    /// The counters as they stand now.
    ///
    /// Flushes first: the periodic reader would otherwise answer with whatever
    /// the last interval happened to capture, which for a run shorter than the
    /// interval is nothing at all.
    ///
    /// # Errors
    ///
    /// [`Error::InstrumentUnreadable`] when the provider will not flush or the
    /// captured reading cannot be taken.
    pub fn read(&self) -> Result<PollCounters> {
        self.provider.force_flush()?;
        self.captured
            .latest
            .lock()
            .map(|latest| *latest)
            .map_err(|_poisoned| Error::InstrumentPoisoned)
    }
}
