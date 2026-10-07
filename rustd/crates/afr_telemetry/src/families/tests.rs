//! The runner census is fed in both directions, every ceiling admits its
//! label product, and every producer reaches its family.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use std::collections::{BTreeMap, BTreeSet};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use afd_observability::metrics::instrument::Instruments;
use afd_observability::metrics::label::http::{DiscardReason, Signal};
use afd_observability::metrics::registry::Policy;
use opentelemetry::metrics::MeterProvider as _;
use opentelemetry_sdk::error::OTelSdkResult;
use opentelemetry_sdk::metrics::data::{AggregatedMetrics, Metric, MetricData, ResourceMetrics};
use opentelemetry_sdk::metrics::exporter::PushMetricExporter;
use opentelemetry_sdk::metrics::{PeriodicReader, SdkMeterProvider, Temporality};

use super::{
    ACTIVITY_FRAMES_DROPPED, Families, MEMORY_PUSH_FAILURES, OTLP_ENTRIES_DISCARDED,
    PROVIDER_RETRIES, PROVIDER_TURN_DURATION, SANDBOX_START_DURATION, SPANS_SUPPRESSED,
    TOOL_CALL_DURATION, TOOL_OUT_OF_MEMORY, registry,
};
use crate::labels::{
    FrameDrop, Provider, PushFailure, RetryReason, SandboxStart, Tool, ToolOutcome, TurnOutcome,
};
use crate::record::Recorder;

/// The instrumentation scope this suite records under.
const SCOPE: &str = "a-test";

/// One family as an export carried it: its name, and the label keys its data
/// points were written with.
type Exported = (String, BTreeSet<String>);

/// An exporter that hands back every family an export carried, with the label
/// keys its producer actually wrote.
#[derive(Debug)]
struct Names(Sender<Exported>);

impl PushMetricExporter for Names {
    fn export(&self, metrics: &ResourceMetrics) -> impl Future<Output = OTelSdkResult> + Send {
        for scope in metrics.scope_metrics() {
            for metric in scope.metrics() {
                let _unread = self.0.send((metric.name().to_owned(), keys_of(metric)));
            }
        }
        std::future::ready(Ok(()))
    }

    fn force_flush(&self) -> OTelSdkResult {
        Ok(())
    }

    fn shutdown_with_timeout(&self, _timeout: Duration) -> OTelSdkResult {
        Ok(())
    }

    fn temporality(&self) -> Temporality {
        Temporality::Cumulative
    }
}

/// The label keys `metric`'s data points carry. The runner's families are
/// `f64` histograms and `u64` counters; any other shape carries none here.
fn keys_of(metric: &Metric) -> BTreeSet<String> {
    let keys: Vec<String> = match metric.data() {
        AggregatedMetrics::F64(MetricData::Histogram(histogram)) => histogram
            .data_points()
            .flat_map(|point| point.attributes().map(|pair| pair.key.to_string()))
            .collect(),
        AggregatedMetrics::U64(MetricData::Sum(sum)) => sum
            .data_points()
            .flat_map(|point| point.attributes().map(|pair| pair.key.to_string()))
            .collect(),
        _other_shape => Vec::new(),
    };
    keys.into_iter().collect()
}

/// An instrument set over the runner census, and the provider it records on.
fn instruments() -> (Instruments, SdkMeterProvider, Receiver<Exported>) {
    let (sent, received) = mpsc::channel();
    let provider = SdkMeterProvider::builder()
        .with_reader(PeriodicReader::builder(Names(sent)).build())
        .build();
    let instruments = Instruments::new(
        registry().expect("the runner census reads"),
        provider.meter(SCOPE),
        provider.meter(SCOPE),
    );
    (instruments, provider, received)
}

/// Every runner census family has a producer, and every producer a row.
///
/// Both directions in one claim: a producer naming a family the census does
/// not declare is refused by the claim, and a row nothing claims is left in
/// `unclaimed`.
#[test]
fn test_every_runner_census_family_has_a_producer() {
    let (instruments, _provider, _names) = instruments();

    let _families =
        Families::claim(&instruments).expect("every producer names a family the census declares");

    assert!(
        instruments.unclaimed().is_empty(),
        "the runner census declares families nothing produces: {:?}",
        instruments.unclaimed()
    );
}

/// Each family, and how many series its closed sets can produce.
fn label_products() -> [(&'static str, usize); 9] {
    [
        (
            PROVIDER_TURN_DURATION.wire_name(),
            Provider::count() * TurnOutcome::ALL.len(),
        ),
        (
            PROVIDER_RETRIES.wire_name(),
            Provider::count() * RetryReason::ALL.len(),
        ),
        (SANDBOX_START_DURATION.wire_name(), SandboxStart::ALL.len()),
        (ACTIVITY_FRAMES_DROPPED.wire_name(), FrameDrop::ALL.len()),
        (MEMORY_PUSH_FAILURES.wire_name(), PushFailure::ALL.len()),
        (
            TOOL_CALL_DURATION.wire_name(),
            Tool::COUNT * ToolOutcome::ALL.len(),
        ),
        (TOOL_OUT_OF_MEMORY.wire_name(), 1),
        (SPANS_SUPPRESSED.wire_name(), 1),
        (
            OTLP_ENTRIES_DISCARDED.wire_name(),
            Signal::ALL.len() * DiscardReason::ALL.len(),
        ),
    ]
}

/// Each family's declared ceiling admits its label product.
///
/// A ceiling under the real count does not drop the excess values: the SDK
/// folds live data into its overflow marker and the panel keeps drawing a
/// line that is now the wrong one.
#[test]
fn test_runner_ceilings_admit_their_label_product() {
    let registry = registry().expect("the runner census reads");
    for (name, product) in label_products() {
        let family = registry
            .family(name)
            .expect("every family named here is declared");
        let Policy::Fixed { max_series } = family.policy else {
            unreachable!("`{name}` carries closed labels and no fixed ceiling");
        };
        assert!(
            max_series >= product,
            "the census admits {max_series} series for `{name}`, whose label sets write {product}"
        );
    }
}

/// Every producer reaches its own family on the export, writing exactly the
/// label keys its census row declares.
///
/// Read off the exported data points rather than a table copied into this
/// file: a producer that wrote one key for another, or dropped one, would
/// still reach its family by name while every series carried a key the census
/// never declared.
#[test]
fn every_producer_writes_the_keys_its_census_row_declares() {
    let (instruments, provider, exported) = instruments();
    let families = Families::claim(&instruments).expect("the census is claimed");
    let elapsed = Duration::from_millis(5);

    families.turn(Provider::of("anthropic"), TurnOutcome::Completed, elapsed);
    families.retry(Provider::of("anthropic"), RetryReason::RateLimited);
    families.sandbox_start(SandboxStart::Ready, elapsed);
    families.frames_dropped(FrameDrop::Backpressure, 3);
    families.push_failed(PushFailure::Upstream);
    families.tool_call(Tool::of("file_read"), ToolOutcome::Succeeded, elapsed);
    families.out_of_memory();
    families.spans_suppressed(2);
    families.export_discarded(Signal::Traces, DiscardReason::ExportRejected, 4);
    provider.force_flush().expect("the reader collects");

    let exported: BTreeMap<String, BTreeSet<String>> = exported.try_iter().collect();
    let registry = registry().expect("the runner census reads");
    for (name, _product) in label_products() {
        let written = exported
            .get(name)
            .unwrap_or_else(|| unreachable!("`{name}` was never exported: {exported:?}"));
        let declared: BTreeSet<String> = registry
            .family(name)
            .expect("declared")
            .labels
            .iter()
            .map(|key| key.as_ref().to_owned())
            .collect();
        assert_eq!(
            written, &declared,
            "`{name}` writes the keys its row declares"
        );
    }
}
