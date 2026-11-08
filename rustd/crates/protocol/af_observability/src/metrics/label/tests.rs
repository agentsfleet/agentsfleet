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
    products.extend([
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
        (
            http::OTLP_ENTRIES_DISCARDED_TOTAL.wire_name(),
            http_labels::Signal::ALL.len() * http_labels::DiscardReason::ALL.len(),
        ),
        (
            http::OTEL_ATTRIBUTE_OMITTED_TOTAL.wire_name(),
            http_labels::OmittedAttribute::ALL.len() * http_labels::OmissionReason::ALL.len(),
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
            fleet::RUNNER_WAKE_ANSWERS_TOTAL.wire_name(),
            fleet_labels::WakeAnswer::ALL.len(),
        ),
        (
            fleet::LEASE_HISTORY_CUTS_TOTAL.wire_name(),
            fleet_labels::HistoryCut::ALL.len(),
        ),
        (
            fleet::LEASE_HELD_CLAIMS_TOTAL.wire_name(),
            fleet_labels::HeldClaim::ALL.len(),
        ),
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
    ]);
    products
}

/// The bus and outbound families M244 added.
fn transport_products() -> [(&'static str, usize); 2] {
    [
        (bus::GAPS.wire_name(), bus_labels::GapCause::ALL.len()),
        (
            outbound::CLAIMS.wire_name(),
            outbound_labels::ClaimVia::ALL.len(),
        ),
    ]
}

fn stream_products() -> [(&'static str, usize); 8] {
    let attempts =
        fleet_labels::StreamTransport::ALL.len() * fleet_labels::StreamOutcome::ALL.len();
    [
        (fleet::RUNNER_STREAM_CONNECT_TOTAL.wire_name(), attempts),
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

/// Every closed set this build declares, with the spellings it can write.
///
/// Named one by one on purpose: a set added to the module and not to this list
/// is a set nothing grades, and deriving the list would only compare the
/// module with itself.
fn every_closed_set() -> Vec<(&'static str, Vec<&'static str>)> {
    macro_rules! set {
        ($path:path) => {{
            use $path as Set;
            (
                stringify!($path),
                Set::ALL.iter().map(|member| member.as_str()).collect(),
            )
        }};
    }
    vec![
        set!(crate::metrics::label::bus::GapCause),
        set!(crate::metrics::label::outbound::ClaimVia),
        set!(crate::metrics::label::fleet::StreamPacket),
        set!(crate::metrics::label::fleet::StreamTransport),
        set!(crate::metrics::label::fleet::StreamOutcome),
        set!(crate::metrics::label::fleet::StreamStage),
        set!(crate::metrics::label::fleet::StreamFallback),
        set!(crate::metrics::label::fleet::WakeQueueOutcome),
        set!(crate::metrics::label::task::TaskOperation),
        set!(crate::metrics::label::task::TaskOutcome),
        set!(crate::metrics::label::database::PoolRole),
        set!(crate::metrics::label::database::PoolState),
        set!(crate::metrics::label::database::AcquireOutcome),
        set!(crate::metrics::label::models::CallOutcome),
        set!(crate::metrics::label::scheduler::FireOutcome),
        set!(crate::metrics::label::scheduler::SyncOutcome),
        set!(crate::metrics::label::scheduler::ScheduleState),
        set!(crate::metrics::label::http::RequestMethod),
        set!(crate::metrics::label::http::ResponseClass),
        set!(crate::metrics::label::cost::TokenType),
        set!(crate::metrics::label::cost::ChargeClass),
        set!(crate::metrics::label::cost::ErrorType),
        set!(crate::metrics::label::fleet::SignupFailure),
        set!(crate::metrics::label::fleet::HistoryCut),
        set!(crate::metrics::label::fleet::HeldClaim),
        set!(crate::metrics::label::fleet::AdmissionOutcome),
        set!(crate::metrics::label::http::Signal),
        set!(crate::metrics::label::http::DiscardReason),
        set!(crate::metrics::label::http::OmittedAttribute),
        set!(crate::metrics::label::http::OmissionReason),
        set!(crate::metrics::label::library::Surface),
        set!(crate::metrics::label::library::Stage),
        set!(crate::metrics::label::library::ReadOutcome),
    ]
}

/// No closed set spells two of its members alike.
///
/// The ceiling test above multiplies `ALL.len()`, so a copy-pasted arm that
/// gives two variants one string understates nothing and still collapses two
/// series into one — two different outcomes drawn as a single line, with the
/// budget still reporting room for both.
#[test]
fn no_closed_set_spells_two_members_alike() {
    for (set, spellings) in every_closed_set() {
        let distinct: std::collections::BTreeSet<&str> = spellings.iter().copied().collect();
        assert_eq!(
            distinct.len(),
            spellings.len(),
            "`{set}` spells two of its {} members alike, which is one series \
             where the ceiling counts two",
            spellings.len()
        );
    }
}
