//! The lease gate's reads and the receive debit, against the rebuilt schema.
//!
//! Each statement is graded on both halves its absence can mean: a workspace
//! with no tenant, a tenant with no wallet, a model the catalogue does not
//! carry. Those are values, never errors, and a reader that turned one into
//! the other would invert a gate.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_billing::charge::Charged;
use afd_billing::rates::Estimate;
use afd_billing::tenant::CHARGES_LIMIT_DEFAULT;
use afd_billing::{
    Cumulative, ESTIMATE_FLOOR_INPUT_TOKENS, ESTIMATE_FLOOR_OUTPUT_TOKENS, Posture, RECEIVE_NANOS,
    RUN_NANOS_PER_SEC, SliceRates, slice_charge,
};
use jiff::Timestamp;

use crate::lane::{FLEET_NAME, Lane, PROVIDER, mint};

/// The instant the receive debits below mint their rows at.
const NOW: Timestamp = Timestamp::constant(1_783_699_440, 0);

/// The event a receive debit charges for was created a minute earlier.
const EVENT_CREATED_AT: Timestamp = Timestamp::constant(1_783_699_380, 0);

/// Three distinct prices, so a column read into the wrong field shows.
const INPUT_PRICE: i64 = 3_000_000_000;
const CACHED_PRICE: i64 = 300_000_000;
const OUTPUT_PRICE: i64 = 15_000_000_000;

/// The wallet balance the snapshot tests open with.
const BALANCE: i64 = 42_000_000_000;

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_resolve_the_payer_and_answer_none_for_an_unknown_workspace() {
    let lane = Lane::create().await;

    let payer = lane.accounts.payer(&lane.workspace).await.expect("answers");
    assert_eq!(payer, Some(lane.tenant.clone()));

    let stranger = lane.accounts.payer(&mint()).await.expect("answers");
    assert_eq!(stranger, None, "a workspace with no row has no payer");
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_read_the_wallet_and_tell_absent_from_empty() {
    let lane = Lane::create().await;
    assert_eq!(
        lane.accounts.wallet(&lane.tenant).await.expect("answers"),
        None,
        "no wallet row is absent, which the gate admits"
    );

    let exhausted = Timestamp::constant(1_783_000_000, 0);
    lane.wallet(0, Some(exhausted), NOW).await;
    let wallet = lane
        .accounts
        .wallet(&lane.tenant)
        .await
        .expect("answers")
        .expect("the wallet row is there");
    assert_eq!(wallet.balance.as_i64(), 0);
    assert_eq!(wallet.exhausted_at, Some(exhausted));
    assert_eq!(wallet.updated_at, NOW);
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_price_a_catalogued_model_and_refuse_to_price_an_unknown_one() {
    let lane = Lane::create().await;
    let model = lane
        .priced_model(INPUT_PRICE, CACHED_PRICE, OUTPUT_PRICE)
        .await;
    let rates = SliceRates {
        run_nanos_per_sec: RUN_NANOS_PER_SEC,
        input_nanos_per_mtok: INPUT_PRICE,
        cached_input_nanos_per_mtok: CACHED_PRICE,
        output_nanos_per_mtok: OUTPUT_PRICE,
    };

    let priced = lane
        .accounts
        .estimate(Posture::Platform, PROVIDER, &model)
        .await
        .expect("answers");
    let floor = slice_charge(
        rates,
        0,
        ESTIMATE_FLOOR_INPUT_TOKENS,
        0,
        ESTIMATE_FLOOR_OUTPUT_TOKENS,
    );
    assert_eq!(priced, Estimate::Priced(floor));

    let unknown = lane
        .accounts
        .estimate(Posture::Platform, PROVIDER, "not-in-the-catalogue")
        .await
        .expect("a miss is a value, not an error");
    assert_eq!(unknown, Estimate::Unpriceable);

    let metered = lane
        .accounts
        .meter(Posture::Platform, PROVIDER, &model, Cumulative::default())
        .await
        .expect("answers");
    assert_eq!(metered.rates, rates, "each price lands in its own field");
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_record_one_receive_row_per_event_with_the_fleet_name() {
    let lane = Lane::create().await;
    let event = mint();
    let charged = Charged {
        tenant_id: &lane.tenant,
        workspace_id: &lane.workspace,
        fleet_id: &lane.fleet,
        event_id: event.as_str(),
        posture: Posture::Platform,
        model: PROVIDER,
        event_created_at: EVENT_CREATED_AT,
    };

    let drained = lane
        .accounts
        .debit_receive(charged, NOW)
        .await
        .expect("the receive charge records");
    assert_eq!(drained, RECEIVE_NANOS);
    // A redelivery reaching the statement again writes nothing new.
    lane.accounts
        .debit_receive(charged, NOW)
        .await
        .expect("the replay is a no-op, not a failure");

    let rows = lane
        .billing
        .charges(&lane.tenant, CHARGES_LIMIT_DEFAULT, None)
        .await
        .expect("answers");
    let [row] =
        <&[_; 1]>::try_from(rows.as_slice()).expect("the replay guard keeps one row per event");
    assert_eq!(row.event_id, event.as_str());
    assert_eq!(row.fleet_name, FLEET_NAME);
    assert_eq!(row.charged_nanos, RECEIVE_NANOS.as_i64());
    assert_eq!(row.input_token_count, None);
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_refuse_a_charge_for_a_fleet_that_has_no_row() {
    let lane = Lane::create().await;
    let missing = mint();
    let event = mint();
    let charged = Charged {
        tenant_id: &lane.tenant,
        workspace_id: &lane.workspace,
        fleet_id: &missing,
        event_id: event.as_str(),
        posture: Posture::Platform,
        model: PROVIDER,
        event_created_at: EVENT_CREATED_AT,
    };

    let refused = lane.accounts.debit_receive(charged, NOW).await;
    assert!(
        refused.is_err(),
        "fleet_name is NOT NULL, so a purged fleet refuses the row"
    );
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_snapshot_the_wallet_and_refuse_a_tenant_without_one() {
    let lane = Lane::create().await;
    let missing = lane.billing.snapshot(&lane.tenant).await;
    assert!(
        missing.is_err(),
        "a missing wallet is a bootstrap violation"
    );

    lane.wallet(BALANCE, None, NOW).await;
    let wallet = lane.billing.snapshot(&lane.tenant).await.expect("answers");
    assert_eq!(wallet.balance.as_i64(), BALANCE);
    assert_eq!(wallet.exhausted_at, None);
    assert_eq!(wallet.updated_at, NOW);
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_meter_at_the_run_fee_alone_when_tokens_are_not_ours_to_price() {
    let lane = Lane::create().await;
    let run_fee_only = SliceRates {
        run_nanos_per_sec: RUN_NANOS_PER_SEC,
        input_nanos_per_mtok: 0,
        cached_input_nanos_per_mtok: 0,
        output_nanos_per_mtok: 0,
    };
    let counted = Cumulative::reported(1, 2, 3);

    // The tenant's own key: tokens land on their provider bill.
    let own_key = lane
        .accounts
        .meter(Posture::SelfManaged, PROVIDER, "any-model", counted)
        .await
        .expect("a self-managed meter needs no catalogue");
    assert_eq!(own_key.rates, run_fee_only);
    assert_eq!(own_key.cumulative, counted);

    // A platform model the catalogue does not carry.
    let uncatalogued = lane
        .accounts
        .meter(Posture::Platform, PROVIDER, "not-in-the-catalogue", counted)
        .await
        .expect("a miss is a value, not an error");
    assert_eq!(uncatalogued.rates, run_fee_only);

    // The fallback a caller takes when the catalogue could not be read.
    assert_eq!(lane.accounts.run_fee_meter(counted).rates, run_fee_only);

    let estimate = lane
        .accounts
        .estimate(Posture::SelfManaged, PROVIDER, "any-model")
        .await
        .expect("a self-managed estimate needs no catalogue");
    assert_eq!(
        estimate,
        Estimate::Priced(slice_charge(run_fee_only, 0, 0, 0, 0))
    );
}
