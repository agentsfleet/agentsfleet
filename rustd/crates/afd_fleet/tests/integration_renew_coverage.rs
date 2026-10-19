//! Renewal coverage decisions that need live rows but no broad lane orchestration.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use crate::integration_held_sandbox::report::processed;
use crate::queue;
use crate::report_seed;
use afd_core::error_code;
use afd_core::test_util::trace::Capture;
use afd_fleet::lease::Plane;
use afd_wire::report::{RenewRequest, ReportRequest};
use jiff::Timestamp;

use crate::seed::{MODEL, PROVIDER};
use crate::support::Fixtures;

use self::report_seed::{DEEP_POOL, Held, held, held_in};

const ONE_NANO_DAILY_BUDGET: &str = r#"{"name":"renew-cover","x-agentsfleet":{"triggers":[],"tools":[],"budget":{"daily_nanos":"1"}}}"#;
const DEEP_DAILY_BUDGET: &str = r#"{"name":"renew-cover","x-agentsfleet":{"triggers":[],"tools":[],"budget":{"daily_nanos":"1000000000000"}}}"#;

/// The status a kill leaves on the fleet row.
///
/// `Leases::installed` answers `Ok(None)` for it, as for every status that is
/// not `active`. The renewal's ceiling read must not: the cases below kill a
/// fleet while its run holds a lease and renew that lease.
const FLEET_KILLED: &str = "killed";

async fn set_fleet_config(held: &Held, config: &str) {
    held.fixtures.native_config(&held.fleet, config).await;
}

async fn remove_wallet(held: &Held) {
    let mut connection = held
        .fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query("DELETE FROM billing.wallets WHERE tenant_id = $1::uuid")
        .bind(&held.tenant)
        .execute(&mut *connection)
        .await
        .expect("the wallet fixture is removed");
}

async fn exhaust_fleet_budget(held: &Held) {
    held.fixtures.seed_wallet(&held.tenant, DEEP_POOL).await;
    set_fleet_config(held, ONE_NANO_DAILY_BUDGET).await;
    let mut connection = held
        .fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query(
        "INSERT INTO billing.usage_lines \
           (id, work_type, work_id, seq, tenant_id, workspace_id, fleet_id, fleet_name, \
            event_id, provider, model, posture, requested_at, input_token_count, \
            cached_input_token_count, cache_write_token_count, cache_write_5m_token_count, \
            cache_write_1h_token_count, output_token_count, runtime_ms, unpriced_token_count, \
            charged_nanos, created_at, updated_at) \
         SELECT $1::uuid, $2, $1::uuid, 0, f.tenant_id, f.workspace_id, f.id, f.name, $3, \
                'anthropic', 'claude-fixture', 'platform', now(), 0, 0, 0, 0, 0, 0, 0, 0, 2, \
                now(), now() \
           FROM fleet.fleets f WHERE f.id = $4::uuid",
    )
    .bind(afd_db::test_util::mint_uuid7().as_str())
    .bind(afd_billing::sql::work::FLEET_LEASE)
    .bind(&held.event_id)
    .bind(&held.fleet)
    .execute(&mut *connection)
    .await
    .expect("the fleet spends past the ceiling");
}

async fn make_budget_unreadable(held: &Held) {
    held.fixtures.corrupt_native_config(&held.fleet, "{}").await;
}

async fn kill_fleet(held: &Held) {
    let mut connection = held
        .fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query("UPDATE fleet.fleets SET status = $2 WHERE id = $1::uuid")
        .bind(&held.fleet)
        .bind(FLEET_KILLED)
        .execute(&mut *connection)
        .await
        .expect("the live fleet is killed");
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn missing_wallet_admits_while_exhausted_and_unreadable_budgets_refuse() {
    let held = held().await;
    let plane = held.fixtures.plane();
    let lease_id = held.issued.lease_id.as_str();

    // The wallet and fleet ceiling are independent gates. Give the latter
    // explicit room so this assertion can prove only the absent-wallet arm;
    // the generic seed's `{}` config is deliberately not a budget fixture.
    set_fleet_config(&held, DEEP_DAILY_BUDGET).await;
    remove_wallet(&held).await;
    plane
        .renew(&held.runner, lease_id, RenewRequest::default(), held.now)
        .await
        .expect("an absent wallet remains the documented fail-open case");

    exhaust_fleet_budget(&held).await;
    let exhausted = plane
        .renew(&held.runner, lease_id, RenewRequest::default(), held.now)
        .await
        .expect_err("a live ceiling at equality refuses the renewal");
    assert_eq!(exhausted.code(), error_code::RUN_BUDGET_EXCEEDED);

    make_budget_unreadable(&held).await;
    let malformed = plane
        .renew(&held.runner, lease_id, RenewRequest::default(), held.now)
        .await
        .expect_err("an unreadable stored ceiling fails closed");
    assert_eq!(malformed.code(), error_code::RUN_BUDGET_EXCEEDED);

    drop(plane);
    queue::clear_ready(held.fixtures.queue(), &held.fleet).await;
    held.fixtures.cleanup().await;
}

/// A fleet killed mid-run keeps the ceiling its run was admitted under.
///
/// Refuses FIRST while the fleet is active, against the same breached ceiling,
/// so the refusal after the kill is proved to come from that ceiling and not
/// from the kill itself.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_fleet_killed_mid_run_keeps_its_breached_ceiling() {
    let held = held().await;
    let plane = held.fixtures.plane();
    let lease_id = held.issued.lease_id.as_str();

    exhaust_fleet_budget(&held).await;
    let refused = plane
        .renew(&held.runner, lease_id, RenewRequest::default(), held.now)
        .await
        .expect_err("the breached ceiling refuses while the fleet is still active");
    assert_eq!(refused.code(), error_code::RUN_BUDGET_EXCEEDED);

    kill_fleet(&held).await;
    let still_refused = plane
        .renew(&held.runner, lease_id, RenewRequest::default(), held.now)
        .await
        .expect_err("a kill does not lift the ceiling of the run in flight");
    assert_eq!(still_refused.code(), error_code::RUN_BUDGET_EXCEEDED);

    drop(plane);
    queue::clear_ready(held.fixtures.queue(), &held.fleet).await;
    held.fixtures.cleanup().await;
}

/// A kill never cancels a run that still has room: the lease in flight renews.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_fleet_killed_mid_run_with_room_still_renews() {
    let held = held().await;
    let plane = held.fixtures.plane();
    let lease_id = held.issued.lease_id.as_str();

    held.fixtures.seed_wallet(&held.tenant, DEEP_POOL).await;
    set_fleet_config(&held, DEEP_DAILY_BUDGET).await;
    kill_fleet(&held).await;
    plane
        .renew(&held.runner, lease_id, RenewRequest::default(), held.now)
        .await
        .expect("a killed fleet's run with room left keeps renewing");

    drop(plane);
    queue::clear_ready(held.fixtures.queue(), &held.fleet).await;
    held.fixtures.cleanup().await;
}

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
    let held = held_in(Fixtures::create_isolated_with_queue().await).await;
    let plane = held.fixtures.plane();
    set_fleet_config(&held, DEEP_DAILY_BUDGET).await;
    held.fixtures.seed_model_rate(PROVIDER, MODEL).await;
    let first = token_charge(&held, &plane, 0, 10_000).await;
    assert!(first > 0, "the fixture rate prices ten thousand tokens");

    renew_through_a_catalogue_fault(&held, &plane, reported(&held, 1, 25_000)).await;
    assert_eq!(
        held.fixtures
            .text(
                crate::rows::Row::Line(&held.event_id, "1"),
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
    queue::clear_ready(held.fixtures.queue(), &held.fleet).await;
    held.fixtures.cleanup().await;
}

/// A report the catalogue fault reaches logs its tokens as unpriced.
///
/// The report's request line carries every token it reported, counted as
/// unpriced: visible rather than lost.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_report_during_a_catalogue_fault_logs_the_tokens_it_leaves_uncharged() {
    let held = held_in(Fixtures::create_isolated_with_queue().await).await;
    set_fleet_config(&held, DEEP_DAILY_BUDGET).await;
    // A priced model, so the refusal below is the fault and not a missing rate.
    held.fixtures.seed_model_rate(PROVIDER, MODEL).await;
    let report = ReportRequest {
        requests: afd_wire::usage::RequestLines(vec![request(&held, 0, 12_000, 3_000, 500)]),
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
                crate::rows::Row::Line(&held.event_id, "0"),
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

    queue::clear_ready(held.fixtures.queue(), &held.fleet).await;
    held.fixtures.cleanup().await;
}

/// One provider request numbered `seq`, made at `held`'s instant.
fn request(
    held: &Held,
    seq: u32,
    input_tokens: u32,
    cached_input_tokens: u32,
    output_tokens: u32,
) -> afd_wire::usage::RequestUsage {
    afd_wire::usage::RequestUsage {
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
        requests: afd_wire::usage::RequestLines(vec![request(held, seq, input_tokens, 0, 0)]),
    }
}

/// The run fee for the runtime between two meter stamps. Renewal stamps the
/// database clock it read, so these are the instants it metered between.
fn run_fee(from: Timestamp, to: Timestamp) -> afd_billing::Nanos {
    let wall_ms = afd_core::clock::saturating_millis_signed(afd_core::clock::elapsed(from, to));
    afd_billing::Nanos::from_i64(report_seed::run_fee(wall_ms))
}

/// Renews `held`'s lease with request `seq` of `input_tokens` and answers what
/// it charged for tokens alone: the charge less the run fee for the runtime it
/// measured.
async fn token_charge(held: &Held, plane: &Plane, seq: u32, input_tokens: u32) -> i64 {
    let from = held.fixtures.metered_since(&held.fleet).await;
    let (_, charged) = plane
        .renew(
            &held.runner,
            held.issued.lease_id.as_str(),
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
async fn renew_through_a_catalogue_fault(held: &Held, plane: &Plane, request: RenewRequest) {
    let lease_id = held.issued.lease_id.as_str();
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
    assert_eq!(warned.field("lease_id"), Some(lease_id));
    drop(log);
    assert_eq!(
        unpriced,
        run_fee(metered_from, metered_to),
        "the run fee for the runtime measured, and no token priced while the catalogue was offline"
    );
    held.fixtures.set_catalogue_readable(true).await;
}
