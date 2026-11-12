//! The census ceilings admit the label sets this build actually writes.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use crate::metrics::declared::{
    billing, bus, database, fleet, http, library, outbound, scheduler, task,
};
use crate::metrics::label::billing as billing_labels;
use crate::metrics::label::bus as bus_labels;
use crate::metrics::label::database as database_labels;
use crate::metrics::label::fleet as fleet_labels;
use crate::metrics::label::http as http_labels;
use crate::metrics::label::library as library_labels;
use crate::metrics::label::outbound as outbound_labels;
use crate::metrics::label::scheduler as scheduler_labels;
use crate::metrics::label::task as task_labels;
use crate::metrics::registry::{Policy, Registry};

/// Where a sent message went, and the folds a live run claimed and settled.
fn fold_products() -> [(&'static str, usize); 2] {
    [
        (
            fleet::STEER_DISPOSITIONS_TOTAL.wire_name(),
            fleet_labels::SteerPlaced::ALL.len(),
        ),
        (
            fleet::LEASE_FOLDS_TOTAL.wire_name(),
            fleet_labels::FoldOutcome::ALL.len(),
        ),
    ]
}

/// Each labelled family and the number of series its closed sets can produce.
///
/// Written out rather than derived, because the pairing IS the claim: which
/// label sets a family carries is a fact about the producer, and a table that
/// derived it from the census would only be checking the census against
/// itself.
fn label_products() -> Vec<(&'static str, usize)> {
    let mut products = pool_schedule_products();
    products.extend(fold_products());
    products.extend(stream_products());
    products.extend(transport_products());
    products.extend(task_products());
    products.extend(billing_products());
    products.extend(http_products());
    products.extend(fleet_products());
    products.extend(library_products());
    products
}

/// The bus and outbound transport families.
fn transport_products() -> [(&'static str, usize); 2] {
    [
        (bus::GAPS.wire_name(), bus_labels::GapCause::ALL.len()),
        (
            outbound::CLAIMS.wire_name(),
            outbound_labels::ClaimVia::ALL.len(),
        ),
    ]
}

fn stream_products() -> [(&'static str, usize); 9] {
    let attempts =
        fleet_labels::StreamTransport::ALL.len() * fleet_labels::StreamOutcome::ALL.len();
    [
        (fleet::RUNNER_STREAM_CONNECT_TOTAL.wire_name(), attempts),
        (
            fleet::RUNNER_STREAM_TLS_HANDSHAKES_TOTAL.wire_name(),
            fleet_labels::StreamTransport::ALL.len()
                * fleet_labels::StreamPeer::ALL.len()
                * fleet_labels::StreamKeyExchange::ALL.len()
                * fleet_labels::StreamTlsOutcome::ALL.len(),
        ),
        (fleet::RUNNER_STREAM_CONNECT_SECONDS.wire_name(), attempts),
        (
            fleet::RUNNER_STREAM_OPERATION_SECONDS.wire_name(),
            fleet_labels::StreamStage::ALL.len() * fleet_labels::StreamOutcome::ALL.len(),
        ),
        (
            fleet::RUNNER_STREAM_FALLBACK_TOTAL.wire_name(),
            fleet_labels::StreamFallback::ALL.len(),
        ),
        (
            fleet::RUNNER_STREAM_TIMEOUTS_TOTAL.wire_name(),
            fleet_labels::StreamStage::ALL.len(),
        ),
        (
            fleet::FLEET_READY_WAKE_QUEUE_EVENTS_TOTAL.wire_name(),
            fleet_labels::WakeQueueOutcome::ALL.len(),
        ),
        (
            fleet::RUNNER_STREAM_NATIVE_RTT_SECONDS.wire_name(),
            fleet_labels::StreamTransport::ALL.len(),
        ),
        (
            fleet::RUNNER_STREAM_NATIVE_PACKETS_TOTAL.wire_name(),
            fleet_labels::StreamPacket::ALL.len(),
        ),
    ]
}

fn pool_schedule_products() -> Vec<(&'static str, usize)> {
    vec![
        (
            scheduler::CALLBACKS.wire_name(),
            scheduler_labels::FireOutcome::ALL.len(),
        ),
        (
            scheduler::FIRES.wire_name(),
            scheduler_labels::FireOutcome::ALL.len(),
        ),
        (
            scheduler::CALLBACK_DURATION.wire_name(),
            scheduler_labels::FireOutcome::ALL.len(),
        ),
        (scheduler::DISPATCH_LAG.wire_name(), 1),
        (
            scheduler::SYNCS.wire_name(),
            scheduler_labels::SyncOutcome::ALL.len(),
        ),
        (
            scheduler::SCHEDULES.wire_name(),
            scheduler_labels::ScheduleState::ALL.len(),
        ),
        (
            database::POOL_CONNECTIONS.wire_name(),
            database_labels::PoolRole::ALL.len() * database_labels::PoolState::ALL.len(),
        ),
        (
            database::POOL_ACQUIRE_DURATION.wire_name(),
            database_labels::PoolRole::ALL.len() * database_labels::AcquireOutcome::ALL.len(),
        ),
        (
            database::POOL_CHECKOUT_PINGS_TOTAL.wire_name(),
            database_labels::PoolRole::ALL.len(),
        ),
    ]
}

/// No family declares a ceiling below the number of series it can write.
///
/// The failure this prevents is silent and total. A ceiling under the real
/// count does not drop the excess label VALUES — the SDK folds them into
/// `otel.metric.overflow`, which `crate::runner` documents as a backstop that
/// must never fire, and the dashboard keeps drawing a line that is now the
/// wrong one. It was under the real count for six families until this test
/// existed.
#[test]
fn every_declared_ceiling_admits_its_label_product() {
    let registry = Registry::declared().expect("the compiled-in census reads");
    for (name, product) in label_products() {
        let family = registry
            .family(name)
            .expect("every family named here is declared");
        let Policy::Fixed { max_series } = family.policy else {
            unreachable!("`{name}` carries closed labels and no fixed ceiling");
        };
        assert!(
            max_series >= product,
            "the census admits {max_series} series for `{name}`, whose closed \
             label sets can write {product}"
        );
    }
}

fn task_products() -> [(&'static str, usize); 3] {
    [
        (
            task::DECISION_RECOVERY_ATTEMPTS_TOTAL.wire_name(),
            task_labels::DecisionRecoveryOutcome::ALL.len(),
        ),
        (
            task::TASK_OPERATIONS_TOTAL.wire_name(),
            task_labels::TaskOperation::ALL.len() * task_labels::TaskOutcome::ALL.len(),
        ),
        (
            task::TASK_OPERATION_SECONDS.wire_name(),
            task_labels::TaskOperation::ALL.len() * task_labels::TaskOutcome::ALL.len(),
        ),
    ]
}

fn billing_products() -> [(&'static str, usize); 4] {
    [
        (
            billing::BILLING_FUNDS_CHECK_TOTAL.wire_name(),
            billing_labels::WorkType::ALL.len() * billing_labels::FundsOutcome::ALL.len(),
        ),
        (
            billing::BILLING_SETTLE_TOTAL.wire_name(),
            billing_labels::WorkType::ALL.len() * billing_labels::SettleOutcome::ALL.len(),
        ),
        (
            billing::BILLING_SWEEPER_RESETTLED_TOTAL.wire_name(),
            billing_labels::WorkType::ALL.len(),
        ),
        (
            billing::BILLING_UNSETTLED_NANOS.wire_name(),
            billing_labels::WorkType::ALL.len(),
        ),
    ]
}

fn http_products() -> [(&'static str, usize); 2] {
    [
        (
            http::OTLP_ENTRIES_DISCARDED_TOTAL.wire_name(),
            http_labels::Signal::ALL.len() * http_labels::DiscardReason::ALL.len(),
        ),
        (
            http::OTEL_ATTRIBUTE_OMITTED_TOTAL.wire_name(),
            http_labels::OmittedAttribute::ALL.len() * http_labels::OmissionReason::ALL.len(),
        ),
    ]
}

fn fleet_products() -> [(&'static str, usize); 9] {
    [
        (
            fleet::LEASE_CACHE_LOOKUPS_TOTAL.wire_name(),
            fleet_labels::LeaseCache::ALL.len() * fleet_labels::CacheOutcome::ALL.len(),
        ),
        (
            fleet::SIGNUP_FAILED_TOTAL.wire_name(),
            fleet_labels::SignupFailure::ALL.len(),
        ),
        (
            fleet::ADMISSIONS_TOTAL.wire_name(),
            fleet_labels::AdmissionOutcome::ALL.len(),
        ),
        (
            fleet::FLEETS.wire_name(),
            fleet_labels::FleetStatusLabel::ALL.len(),
        ),
        (
            fleet::FLEET_RUNS_STARTED_TOTAL.wire_name(),
            fleet_labels::RunStart::ALL.len(),
        ),
        (
            fleet::FLEET_DELIVERY_STAGE_SECONDS.wire_name(),
            fleet_labels::DeliveryStage::ALL.len(),
        ),
        (
            fleet::RUNNER_REPORT_ANSWERS_TOTAL.wire_name(),
            fleet_labels::ReportAnswer::ALL.len(),
        ),
        (
            fleet::LEASE_HISTORY_CUTS_TOTAL.wire_name(),
            fleet_labels::HistoryCut::ALL.len(),
        ),
        (
            fleet::LEASE_HELD_CLAIMS_TOTAL.wire_name(),
            fleet_labels::HeldClaim::ALL.len(),
        ),
    ]
}

fn library_products() -> [(&'static str, usize); 5] {
    [
        (
            library::LIBRARY_STAGE_DURATION_SECONDS_TOTAL.wire_name(),
            library_labels::Surface::ALL.len() * library_labels::Stage::ALL.len(),
        ),
        (
            library::LIBRARY_STAGE_OBSERVATIONS_TOTAL.wire_name(),
            library_labels::Surface::ALL.len() * library_labels::Stage::ALL.len(),
        ),
        (
            library::LIBRARY_READ_OUTCOME_TOTAL.wire_name(),
            library_labels::Surface::ALL.len() * library_labels::ReadOutcome::ALL.len(),
        ),
        (
            library::LIBRARY_PAYLOAD_BYTES_TOTAL.wire_name(),
            library_labels::Surface::ALL.len(),
        ),
        (
            library::LIBRARY_RESULTS_TOTAL.wire_name(),
            library_labels::Surface::ALL.len(),
        ),
    ]
}
