//! Rate catalogue outages preserve unpriced token observations.

use super::{DEEP_DAILY_BUDGET, set_fleet_config};
use crate::integration_held_sandbox::report::processed;
use crate::report_seed::{self, Held, held_in};
use crate::seed::{MODEL, PROVIDER};
use crate::support::Fixtures;
use af_core::test_util::trace::Capture;
use af_wire::report::{RenewRequest, ReportRequest};
use afd_fleet::lease::LeaseService;
use jiff::Timestamp;

/// A request a renewal could not price is logged unpriced, and the next
/// request prices on its own line.
///
/// Every request is its own line, so nothing is held for a later slice: the
/// request priced during the fault carries its tokens as unpriced, visible on
/// the charges page, and the next one is charged as if the fault never
/// happened. Renewals at one instant, so every nano a token charge counts is a
/// token's.
///
/// A private database, because taking the catalogue offline breaks the rate
/// read for every test sharing it.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_renewal_during_a_catalogue_fault_logs_its_tokens_unpriced() {
    let held = held_in(Fixtures::create_isolated().await).await;
    let plane = held.fixtures.plane();
    set_fleet_config(&held, DEEP_DAILY_BUDGET).await;
    held.fixtures.seed_model_rate(PROVIDER, MODEL).await;
    let first = token_charge(&held, &plane, 0, 10_000).await;
    assert!(first > 0, "the fixture rate prices ten thousand tokens");

    renew_through_a_catalogue_fault(&held, &plane, reported(&held, 1, 25_000)).await;
    assert_eq!(
        held.fixtures
            .text(
                crate::rows::Row::ModelLine(&held.event_id, "1"),
                "unpriced_token_count"
            )
            .await,
        Some("25000".to_owned()),
        "the request priced during the fault carries its tokens as unpriced"
    );

    let later = token_charge(&held, &plane, 2, 40_000).await;
    assert_eq!(
        later,
        4 * first,
        "a later request prices its own tokens and nothing held from the fault"
    );

    drop(plane);
    held.fixtures.cleanup().await;
}

/// A report the catalogue fault reaches logs its tokens as unpriced.
///
/// The report's request line carries every token it reported, counted as
/// unpriced: visible rather than lost.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_report_during_a_catalogue_fault_logs_the_tokens_it_leaves_uncharged() {
    let held = held_in(Fixtures::create_isolated().await).await;
    set_fleet_config(&held, DEEP_DAILY_BUDGET).await;
    // A priced model, so the refusal below is the fault and not a missing rate.
    held.fixtures.seed_model_rate(PROVIDER, MODEL).await;
    let report = ReportRequest {
        folded_through_sequence: None,
        requests: af_wire::usage::RequestLines(vec![request(&held, 0, 12_000, 3_000, 500)]),
        ..processed(&held, None)
    };

    held.fixtures.set_catalogue_readable(false).await;
    let log = Capture::install();
    held.fixtures
        .plane()
        .report(&held.runner, &report, held.now)
        .await
        .expect("a catalogue fault does not stop the report");
    let warned = log.only("lease_tokens_unpriced");
    assert_eq!(
        held.fixtures
            .text(
                crate::rows::Row::ModelLine(&held.event_id, "0"),
                "unpriced_token_count"
            )
            .await,
        Some("15500".to_owned()),
        "every token of the final slice is counted unpriced, none charged"
    );
    assert!(
        warned
            .field("error_code")
            .is_some_and(|code| !code.is_empty())
    );
    assert_eq!(
        warned.field("lease_id"),
        Some(held.issued.lease_id.as_str())
    );
    drop(log);
    held.fixtures.set_catalogue_readable(true).await;

    held.fixtures.cleanup().await;
}

/// One provider request numbered `seq`, made at `held`'s instant.
const fn request(
    held: &Held,
    seq: u32,
    input_tokens: u32,
    cached_input_tokens: u32,
    output_tokens: u32,
) -> af_wire::usage::RequestUsage {
    af_wire::usage::RequestUsage {
        seq,
        at: held.now,
        input_tokens,
        cached_input_tokens,
        cache_write_tokens: 0,
        cache_write_5m_tokens: 0,
        cache_write_1h_tokens: 0,
        output_tokens,
    }
}

/// A renewal carrying one request of `input_tokens`, numbered `seq`.
fn reported(held: &Held, seq: u32, input_tokens: u32) -> RenewRequest {
    RenewRequest {
        requests: af_wire::usage::RequestLines(vec![request(held, seq, input_tokens, 0, 0)]),
    }
}

/// The run fee for the runtime between two meter stamps. Renewal stamps the
/// database clock it read, so these are the instants it metered between.
fn run_fee(from: Timestamp, to: Timestamp) -> afd_billing::Nanos {
    let wall_ms = af_core::clock::saturating_millis_signed(af_core::clock::elapsed(from, to));
    afd_billing::Nanos::from_i64(report_seed::run_fee(wall_ms))
}

/// Renews `held`'s lease with request `seq` of `input_tokens` and answers what
/// it charged for tokens alone: the charge less the run fee for the runtime it
/// measured.
async fn token_charge(held: &Held, plane: &LeaseService, seq: u32, input_tokens: u32) -> i64 {
    let from = held.fixtures.metered_since(&held.fleet).await;
    let (_, charged) = plane
        .renew(
            &held.runner,
            &held.issued.lease_id,
            reported(held, seq, input_tokens),
            held.now,
        )
        .await
        .expect("a priced renewal");
    let to = held.fixtures.metered_since(&held.fleet).await;
    charged.as_i64() - run_fee(from, to).as_i64()
}

/// Renews `held`'s lease for `request` with the catalogue offline, and checks
/// the renewal charged the runtime it measured and no token, and said itself
/// that it logged those tokens unpriced, not only the pricing helper it calls.
async fn renew_through_a_catalogue_fault(held: &Held, plane: &LeaseService, request: RenewRequest) {
    let lease_id = &held.issued.lease_id;
    held.fixtures.set_catalogue_readable(false).await;
    let log = Capture::install();
    let metered_from = held.fixtures.metered_since(&held.fleet).await;
    let (_, unpriced) = plane
        .renew(&held.runner, lease_id, request, held.now)
        .await
        .expect("a catalogue fault does not stop the run");
    // The renewal meters runtime up to the database clock it read, and stamps
    // that instant as the meter's next start.
    let metered_to = held.fixtures.metered_since(&held.fleet).await;
    let warned = log.only("lease_tokens_unpriced");
    assert_eq!(warned.field("lease_id"), Some(lease_id.as_str()));
    drop(log);
    assert_eq!(
        unpriced,
        run_fee(metered_from, metered_to),
        "the run fee for the runtime measured, and no token priced while the catalogue was offline"
    );
    held.fixtures.set_catalogue_readable(true).await;
}
