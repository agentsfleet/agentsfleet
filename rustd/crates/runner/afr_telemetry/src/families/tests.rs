//! The runner census is fed in both directions, every ceiling admits its
//! label product, and every producer reaches its family.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use std::collections::{BTreeMap, BTreeSet};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use af_observability::metrics::instrument::Instruments;
use af_observability::metrics::label::http::{DiscardReason, Signal};
use af_observability::metrics::registry::Policy;
use opentelemetry::metrics::MeterProvider as _;
use opentelemetry_sdk::error::OTelSdkResult;
use opentelemetry_sdk::metrics::data::{AggregatedMetrics, Metric, MetricData, ResourceMetrics};
use opentelemetry_sdk::metrics::exporter::PushMetricExporter;
use opentelemetry_sdk::metrics::{PeriodicReader, SdkMeterProvider, Temporality};

use super::{
    ACTIVITY_FRAMES_DROPPED, DECISION_DURATION, DECISION_EVALUATIONS, Families,
    MEMORY_PUSH_FAILURES, OTLP_ENTRIES_DISCARDED, PROVIDER_RETRIES, PROVIDER_TURN_DURATION,
    SANDBOX_BUILDS, SANDBOX_HOLDS, SANDBOX_RELEASE_DURATION, SANDBOX_START_DURATION,
    SANDBOX_WAIT_DURATION, SPANS_SUPPRESSED, TOOL_CALL_DURATION, TOOL_OUT_OF_MEMORY, registry,
};
use crate::labels::decision::{DecisionOutcome, DecisionProvider, DecisionPurpose};
use crate::labels::{
    FrameDrop, Provider, PushFailure, RetryReason, SandboxBuild, SandboxHold, SandboxRelease,
    SandboxStart, SandboxWait, Tool, ToolOutcome, TurnOutcome,
};
use crate::record::Recorder;

/// The instrumentation scope this suite records under.
const SCOPE: &str = "a-test";

/// One family as an export carried it, with its actual attribute pairs.
type Attributes = BTreeSet<(String, String)>;
type Exported = (String, Attributes);

/// An exporter that hands back every family an export carried, with the label
/// attributes its producer actually wrote.
#[derive(Debug)]
struct Names(Sender<Exported>);

impl PushMetricExporter for Names {
    fn export(&self, metrics: &ResourceMetrics) -> impl Future<Output = OTelSdkResult> + Send {
        for scope in metrics.scope_metrics() {
            for metric in scope.metrics() {
                let _unread = self
                    .0
                    .send((metric.name().to_owned(), attributes_of(metric)));
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

/// The label attributes `metric`'s data points carry. The runner's families are
/// `f64` histograms and `u64` counters; any other shape carries none here.
fn attributes_of(metric: &Metric) -> Attributes {
    let keys: Vec<(String, String)> = match metric.data() {
        AggregatedMetrics::F64(MetricData::Histogram(histogram)) => histogram
            .data_points()
            .flat_map(|point| {
                point
                    .attributes()
                    .map(|pair| (pair.key.to_string(), pair.value.to_string()))
            })
            .collect(),
        AggregatedMetrics::U64(MetricData::Sum(sum)) => sum
            .data_points()
            .flat_map(|point| {
                point
                    .attributes()
                    .map(|pair| (pair.key.to_string(), pair.value.to_string()))
            })
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
fn label_products() -> [(&'static str, usize); 16] {
    [
        (
            DECISION_EVALUATIONS.wire_name(),
            DecisionProvider::ALL.len() * DecisionPurpose::ALL.len() * DecisionOutcome::ALL.len(),
        ),
        (
            DECISION_DURATION.wire_name(),
            DecisionProvider::ALL.len() * DecisionPurpose::ALL.len() * DecisionOutcome::ALL.len(),
        ),
        (
            super::MEMORY_SOURCES.wire_name(),
            crate::labels::MemorySource::ALL.len(),
        ),
        (
            PROVIDER_TURN_DURATION.wire_name(),
            Provider::count() * TurnOutcome::ALL.len(),
        ),
        (
            PROVIDER_RETRIES.wire_name(),
            Provider::count() * RetryReason::ALL.len(),
        ),
        (SANDBOX_START_DURATION.wire_name(), SandboxStart::ALL.len()),
        (SANDBOX_WAIT_DURATION.wire_name(), SandboxWait::ALL.len()),
        (
            SANDBOX_RELEASE_DURATION.wire_name(),
            SandboxRelease::ALL.len(),
        ),
        (SANDBOX_BUILDS.wire_name(), SandboxBuild::ALL.len()),
        (ACTIVITY_FRAMES_DROPPED.wire_name(), FrameDrop::ALL.len()),
        (MEMORY_PUSH_FAILURES.wire_name(), PushFailure::ALL.len()),
        (
            TOOL_CALL_DURATION.wire_name(),
            Tool::COUNT * ToolOutcome::ALL.len(),
        ),
        (TOOL_OUT_OF_MEMORY.wire_name(), 1),
        (SANDBOX_HOLDS.wire_name(), SandboxHold::ALL.len()),
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
    let listed: BTreeSet<&str> = label_products().iter().map(|(name, _)| *name).collect();
    let declared: BTreeSet<&str> = registry
        .families()
        .filter(|family| matches!(family.policy, Policy::Fixed { .. }))
        .map(|family| &*family.name)
        .collect();
    assert_eq!(
        listed, declared,
        "every census family has its label product checked"
    );
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
    record_every_family(&families);
    provider.force_flush().expect("the reader collects");

    let exported: BTreeMap<String, Attributes> = exported.try_iter().collect();
    assert_serving_provider(&exported);
    let registry = registry().expect("the runner census reads");
    for family in registry.families() {
        let name = family.name.as_ref();
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
        let keys: BTreeSet<String> = written.iter().map(|(key, _)| key.clone()).collect();
        assert_eq!(keys, declared, "`{name}` writes the keys its row declares");
    }
}

fn record_every_family(families: &Families) {
    let elapsed = Duration::from_millis(5);

    families.decision(
        DecisionProvider::Typesafe,
        DecisionPurpose::Goal,
        DecisionOutcome::Completed,
        elapsed,
    );
    families.memory_source(crate::labels::MemorySource::Inline);
    families.memory_source(crate::labels::MemorySource::Fallback);
    families.turn(Provider::of("fireworks"), TurnOutcome::Completed, elapsed);
    families.model_call(
        "accounts/fireworks/models/minimax-m3p1-flash",
        Provider::of("fireworks"),
        crate::labels::CallOutcome::Completed,
        elapsed,
        Some(elapsed),
    );
    families.model_tokens(
        "accounts/fireworks/models/minimax-m3p1-flash",
        Provider::of("fireworks"),
        af_wire::usage::ObservedTokens {
            input: Some(10),
            output: Some(3),
            cache_read: Some(4),
            cache_write: Some(2),
        },
    );
    families.retry(Provider::of("anthropic"), RetryReason::RateLimited);
    families.sandbox_start(SandboxStart::Ready, elapsed);
    families.sandbox_wait(SandboxWait::Ready, elapsed);
    families.sandbox_release(SandboxRelease::Released, elapsed);
    families.sandbox_build(SandboxBuild::Checkout);
    families.frames_dropped(FrameDrop::Backpressure, 3);
    families.push_failed(PushFailure::Upstream);
    families.tool_call(Tool::of("file_read"), ToolOutcome::Succeeded, elapsed);
    families.out_of_memory();
    families.sandbox_hold(SandboxHold::Parked);
    families.spans_suppressed(2);
    families.export_discarded(Signal::Traces, DiscardReason::ExportRejected, 4);
}

fn assert_serving_provider(exported: &BTreeMap<String, Attributes>) {
    for (family, key) in [
        (
            "agentsfleet_runner_provider_turn_duration_seconds",
            "provider",
        ),
        (
            "agentsfleet_runner_model_call_duration_seconds",
            "gen_ai.provider.name",
        ),
        (
            "agentsfleet_runner_model_first_token_duration_seconds",
            "gen_ai.provider.name",
        ),
        (
            "agentsfleet_runner_model_tokens_total",
            "gen_ai.provider.name",
        ),
    ] {
        assert!(
            exported
                .get(family)
                .is_some_and(|attributes| attributes.contains(&(key.into(), "fireworks".into()))),
            "{family} must retain the actual serving provider"
        );
    }
}

#[test]
fn model_families_use_the_shared_cost_ceiling() {
    let registry = registry().expect("the runner census reads");
    let expected: BTreeSet<_> = [
        super::MODEL_CALL_DURATION.wire_name(),
        super::MODEL_FIRST_TOKEN.wire_name(),
        super::MODEL_TOKENS.wire_name(),
        super::MODEL_CACHE_READ.wire_name(),
        super::MODEL_CACHE_WRITE.wire_name(),
    ]
    .into_iter()
    .collect();
    let actual: BTreeSet<_> = registry
        .families()
        .filter(|family| matches!(family.policy, Policy::SharedCost))
        .map(|family| family.name.as_ref())
        .collect();
    assert_eq!(actual, expected);
}
