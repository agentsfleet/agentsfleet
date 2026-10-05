//! The runner census is fed in both directions, every ceiling admits its
//! label product, and every producer reaches its family.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use std::collections::BTreeSet;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use afd_observability::metrics::instrument::Instruments;
use afd_observability::metrics::registry::Policy;
use afd_observability::semconv::{LABEL_OUTCOME, LABEL_REASON};
use opentelemetry::metrics::MeterProvider as _;
use opentelemetry_sdk::error::OTelSdkResult;
use opentelemetry_sdk::metrics::data::ResourceMetrics;
use opentelemetry_sdk::metrics::exporter::PushMetricExporter;
use opentelemetry_sdk::metrics::{PeriodicReader, SdkMeterProvider, Temporality};

use super::{
    ACTIVITY_FRAMES_DROPPED, Families, MEMORY_PUSH_FAILURES, PROVIDER_RETRIES,
    PROVIDER_TURN_DURATION, SANDBOX_START_DURATION, SPANS_SUPPRESSED, TOOL_CALL_DURATION, registry,
};
use crate::labels::{
    FrameDrop, LABEL_PROVIDER, LABEL_TOOL, Provider, PushFailure, RetryReason, SandboxStart, Tool,
    ToolOutcome, TurnOutcome,
};
use crate::record::Recorder;

/// The instrumentation scope this suite records under.
const SCOPE: &str = "a-test";

/// An exporter that hands back the name of every family an export carried.
#[derive(Debug)]
struct Names(Sender<String>);

impl PushMetricExporter for Names {
    fn export(&self, metrics: &ResourceMetrics) -> impl Future<Output = OTelSdkResult> + Send {
        for scope in metrics.scope_metrics() {
            for metric in scope.metrics() {
                let _unread = self.0.send(metric.name().to_owned());
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

/// An instrument set over the runner census, and the provider it records on.
fn instruments() -> (Instruments, SdkMeterProvider, Receiver<String>) {
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

/// Each family, the label keys its producer writes, and how many series its
/// closed sets can produce.
fn label_sets() -> [(&'static str, Vec<&'static str>, usize); 7] {
    [
        (
            PROVIDER_TURN_DURATION.wire_name(),
            vec![LABEL_PROVIDER, LABEL_OUTCOME],
            Provider::COUNT * TurnOutcome::ALL.len(),
        ),
        (
            PROVIDER_RETRIES.wire_name(),
            vec![LABEL_PROVIDER, LABEL_REASON],
            Provider::COUNT * RetryReason::ALL.len(),
        ),
        (
            SANDBOX_START_DURATION.wire_name(),
            vec![LABEL_OUTCOME],
            SandboxStart::ALL.len(),
        ),
        (
            ACTIVITY_FRAMES_DROPPED.wire_name(),
            vec![LABEL_REASON],
            FrameDrop::ALL.len(),
        ),
        (
            MEMORY_PUSH_FAILURES.wire_name(),
            vec![LABEL_REASON],
            PushFailure::ALL.len(),
        ),
        (
            TOOL_CALL_DURATION.wire_name(),
            vec![LABEL_TOOL, LABEL_OUTCOME],
            Tool::COUNT * ToolOutcome::ALL.len(),
        ),
        (SPANS_SUPPRESSED.wire_name(), Vec::new(), 1),
    ]
}

/// Each family's declared ceiling admits its label product, and the census
/// names the label keys the producer actually writes.
///
/// A ceiling under the real count does not drop the excess values: the SDK
/// folds live data into its overflow marker and the panel keeps drawing a
/// line that is now the wrong one.
#[test]
fn test_runner_ceilings_admit_their_label_product() {
    let registry = registry().expect("the runner census reads");
    for (name, keys, product) in label_sets() {
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
        let declared: Vec<&str> = family.labels.iter().map(AsRef::as_ref).collect();
        assert_eq!(
            declared, keys,
            "`{name}` declares the keys its producer writes"
        );
    }
}

/// Every producer reaches its own family on the export.
#[test]
fn every_producer_reaches_its_family() {
    let (instruments, provider, names) = instruments();
    let families = Families::claim(&instruments).expect("the census is claimed");
    let elapsed = Duration::from_millis(5);

    families.turn(Provider::of("anthropic"), TurnOutcome::Completed, elapsed);
    families.retry(Provider::of("anthropic"), RetryReason::RateLimited);
    families.sandbox_start(SandboxStart::Ready, elapsed);
    families.frames_dropped(FrameDrop::Backpressure, 3);
    families.push_failed(PushFailure::Upstream);
    families.tool_call(Tool::of("file_read"), ToolOutcome::Succeeded, elapsed);
    families.spans_suppressed(2);
    provider.force_flush().expect("the reader collects");

    let exported: BTreeSet<String> = names.try_iter().collect();
    for (name, _keys, _product) in label_sets() {
        assert!(
            exported.contains(name),
            "`{name}` was never exported: {exported:?}"
        );
    }
}
