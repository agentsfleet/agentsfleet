//! Every producer, driven once through an installed set.
//!
//! # Why this exists as a test at all
//!
//! `every_census_family_has_a_producer` proves a producer is CLAIMED for each
//! declared family. It cannot prove one ever fires, and it says so. The gap it
//! leaves is not theoretical: every producer body is guarded by
//! `if let Some(producers) = installed()`, and nothing in the unit suite
//! installs, so each of those bodies was unreached — a producer that panicked
//! on its own label set, or added to the wrong instrument, would have looked
//! exactly as healthy as one that worked.
//!
//! This installs once and calls all of them. It asserts no counter value:
//! reading a `Counter` back is not something the SDK offers without an
//! exporter, and the claim here is narrower and still worth holding — every
//! producer runs its real body, against every variant of every closed label
//! set it accepts, without panicking and without tripping the instrument
//! layer's own kind and cardinality checks.
//!
//! # Why the integration binary and not the unit suite
//!
//! `INSTALLED` is a process-wide `OnceLock`. Installing inside the unit suite
//! would flip `installed()` to `Some` for every other test in that binary,
//! mid-run and in whatever order the harness chose. Here the process is this
//! file's own, so the install is total and ordered.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use core::time::Duration;

use afd_observability::metrics::instrument::Instruments;
use afd_observability::metrics::label::cost::{ChargeClass, ErrorType};
use afd_observability::metrics::label::fleet::{SignupFailure, SyntheticEvent, VerifierRun};
use afd_observability::metrics::label::http::{
    DiscardReason, OmissionReason, OmittedAttribute, Signal,
};
use afd_observability::metrics::label::library::{ReadOutcome, Stage, Surface};
use afd_observability::metrics::registry::Registry;
use afd_observability::producers::cost::Spend;
use afd_observability::producers::{self, GaugeSources};
use afd_observability::runner::OVERFLOW_RUNNER;
use afd_observability::semconv;
use afd_wire::report::FailureClass;
use opentelemetry::metrics::MeterProvider as _;
use opentelemetry_sdk::metrics::SdkMeterProvider;

/// Installs the process-wide set, over a provider that exports nowhere.
///
/// The answer is dropped deliberately. `false` means a sibling test installed
/// first, which `producers::install` documents as the ordinary case in a test
/// binary — what matters to every assertion here is that `installed()` returns
/// `Some` afterwards, not which call put it there.
fn install() {
    let provider = SdkMeterProvider::builder().build();
    let instruments = Instruments::new(
        Registry::declared().expect("the compiled-in census reads"),
        provider.meter(semconv::SCOPE_NAME),
        provider.meter(semconv::SCOPE_NAME),
    );
    let _took = producers::install(&instruments, &GaugeSources::silent())
        .expect("the compiled-in census claims");
}

/// A runner id short enough to stay under the per-runner table's capacity.
const RUNNER: &str = "runner-drive-fixture";

/// A resident-set reading, in bytes. Any plausible magnitude does: the cell
/// stores what it is handed and this test asserts it did not panic doing so.
const RESIDENT_BYTES: u64 = 67_108_864;

/// A spend, in nanocredits. Carried by the producer, never interpreted here.
const NANOCREDITS: u64 = 1_000_000;

/// Every fleet producer fires, including all six signup refusals.
#[test]
fn test_every_fleet_producer_runs_its_body() {
    install();

    producers::fleet::ready_depth_observed(3);
    producers::fleet::repair_backlog_observed(7, 42);
    producers::fleet::signup_bootstrapped();
    producers::fleet::signup_replayed();
    producers::fleet::lease_polled(11, 2);
    producers::fleet::ready_write_failed();
    producers::fleet::retention_swept(9);
    producers::fleet::retention_failed();

    // Every arm, because the label is what distinguishes them and a variant
    // added without a spelling is the failure this catches.
    for reason in [
        SignupFailure::BadSignature,
        SignupFailure::StaleTimestamp,
        SignupFailure::MissingEmail,
        SignupFailure::DatabaseError,
        SignupFailure::PoolUnavailable,
        SignupFailure::MetadataWriteback,
    ] {
        producers::fleet::signup_failed(reason);
    }
}

/// Every repair producer fires, over both closed outcome sets.
#[test]
fn test_every_repair_producer_runs_its_body() {
    install();

    producers::fleet::repair::dispatch_retried();
    for outcome in [SyntheticEvent::Emitted, SyntheticEvent::Replayed] {
        producers::fleet::repair::event(outcome);
    }
    for outcome in [VerifierRun::Queued, VerifierRun::Completed] {
        producers::fleet::repair::run(outcome);
    }
}

/// Every per-runner producer fires, over all eleven failure classes.
///
/// `failed` also carries the only branch in the module: a runner id past the
/// table's capacity is attributed to `OVERFLOW_RUNNER` and counted twice, once
/// as a failure and once as an overflow. Both sides are driven — the named
/// runner for the ordinary path and the overflow spelling for the other.
#[test]
fn test_every_runner_producer_runs_its_body() {
    install();

    producers::fleet::runner::processed(RUNNER);
    producers::fleet::runner::seen(RUNNER, 1_767_225_600_000);
    producers::fleet::runner::lease_taken(RUNNER);
    producers::fleet::runner::lease_released(RUNNER);

    // No class at all is its own arm — an unmodelled reason still counts.
    producers::fleet::runner::failed(RUNNER, None);
    for class in [
        FailureClass::StartupPosture,
        FailureClass::PolicyDeny,
        FailureClass::TimeoutKill,
        FailureClass::OomKill,
        FailureClass::ResourceKill,
        FailureClass::RunnerCrash,
        FailureClass::TransportLoss,
        FailureClass::LandlockDeny,
        FailureClass::LeaseExpired,
        FailureClass::RenewalTerminate,
        FailureClass::BudgetBreach,
    ] {
        producers::fleet::runner::failed(RUNNER, Some(class));
    }

    // The overflow attribution, which is the one conditional in `failed`.
    producers::fleet::runner::failed(OVERFLOW_RUNNER, Some(FailureClass::RunnerCrash));
}

/// Every http producer fires, over all three signals and both omission sets.
#[test]
fn test_every_http_producer_runs_its_body() {
    install();

    producers::http::request_shed();
    producers::http::stream_shed();
    producers::http::frame_dropped();
    producers::http::hub_reconnected();

    for signal in [Signal::Logs, Signal::Traces, Signal::Metrics] {
        for reason in [
            DiscardReason::RingFull,
            DiscardReason::AggregateCap,
            DiscardReason::SerializeFailed,
            DiscardReason::PartialRejected,
            DiscardReason::ExportRejected,
            DiscardReason::ExportUncertain,
        ] {
            producers::http::export_discarded(signal, reason, 2);
        }
    }

    for attribute in [
        OmittedAttribute::ProviderName,
        OmittedAttribute::RequestModel,
    ] {
        for reason in [
            OmissionReason::UnmappedProvider,
            OmissionReason::BudgetExhausted,
            OmissionReason::ValueTooLong,
        ] {
            producers::http::attribute_omitted(attribute, reason);
        }
    }
}

/// Every memory producer fires, and the resident cell takes both answers.
///
/// `resident_observed(None)` is the withdrawal path, which is a GAP rather
/// than a zero: a process whose resident set could not be read has not shrunk
/// to nothing. It is a distinct arm and gets driven as one.
#[test]
fn test_every_memory_producer_runs_its_body() {
    install();

    producers::memory::hydration_window(12, 3, 4_096);
    producers::memory::captured(5);
    producers::memory::push_failed();
    producers::memory::hydration_dropped(2, 512);
    producers::memory::cap_evicted(1);
    producers::memory::capture_truncated(4);
    producers::memory::capture_skipped(6);
    producers::memory::search_found_nothing();

    producers::memory::resident_observed(Some(RESIDENT_BYTES));
    producers::memory::resident_observed(None);
}

/// Every library producer fires, across all surfaces, stages and outcomes.
#[test]
fn test_every_library_producer_runs_its_body() {
    install();

    let surfaces = [
        Surface::TenantModels,
        Surface::GlobalModels,
        Surface::FleetSummary,
    ];
    for surface in surfaces {
        for stage in [
            Stage::NextUpstream,
            Stage::AuthVerify,
            Stage::PoolWait,
            Stage::Authorize,
            Stage::Sql,
            Stage::SecretProject,
            Stage::Map,
            Stage::Serialize,
            Stage::CacheRevision,
            Stage::CacheLookup,
        ] {
            producers::library::stage_observed(surface, stage, Duration::from_millis(3));
        }
        for outcome in [
            ReadOutcome::Ok,
            ReadOutcome::Invalid,
            ReadOutcome::Unauthorized,
            ReadOutcome::Forbidden,
            ReadOutcome::NotFound,
            ReadOutcome::Timeout,
            ReadOutcome::Cancelled,
            ReadOutcome::DependencyError,
            ReadOutcome::InternalError,
        ] {
            producers::library::read_finished(surface, outcome);
        }
        producers::library::read_served(surface, 25);
        producers::library::payload_served(surface, 8_192);
    }
}

/// Every cost producer fires, on a clean run and an errored one.
///
/// The error arm is the branch: `invocation` pushes an extra attribute only
/// when the run carries a verdict, so a clean spend and a failed one are two
/// different attribute sets out of one function.
#[test]
fn test_every_cost_producer_runs_its_body() {
    install();

    for error in [None, Some(ErrorType::FleetError)] {
        producers::cost::invocation(&Spend {
            model: "claude-opus-5",
            posture: "sandboxed",
            input_tokens: 900,
            cached_input_tokens: 400,
            output_tokens: 150,
            wall: Duration::from_millis(1_250),
            error,
        });
    }

    for class in [
        ChargeClass::Receive,
        ChargeClass::Renewal,
        ChargeClass::Settle,
    ] {
        producers::cost::credits_consumed("claude-opus-5", "sandboxed", class, NANOCREDITS);
    }
}
