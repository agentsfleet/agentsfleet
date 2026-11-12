//! The money reads, against the rebuilt schema.
//!
//! Each is graded on both halves its absence can mean: a model the catalogue
//! does not carry is a value, never an error, and a tenant with no wallet is a
//! refusal, never a zero balance.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_billing::rates::Estimate;
use afd_billing::{ESTIMATE_FLOOR_INPUT_TOKENS, ESTIMATE_FLOOR_OUTPUT_TOKENS, Nanos, Posture};
use jiff::Timestamp;

use crate::lane::{Lane, PROVIDER};

/// The instant the estimates below are priced at and the wallets stamped with.
const NOW: Timestamp = Timestamp::constant(1_783_699_440, 0);

/// Tokens in the unit a catalogue rate is quoted per.
const PER_MTOK: i64 = 1_000_000;

/// Three distinct prices, so a column read into the wrong field shows.
const INPUT_PRICE: i64 = 3_000_000_000;
const CACHED_PRICE: i64 = 300_000_000;
const OUTPUT_PRICE: i64 = 15_000_000_000;

/// The wallet balance the snapshot tests open with.
const BALANCE: i64 = 42_000_000_000;

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_price_a_catalogued_model_and_refuse_to_price_an_unknown_one() {
    let lane = Lane::create().await;
    let model = lane
        .priced_model(INPUT_PRICE, CACHED_PRICE, OUTPUT_PRICE)
        .await;

    let priced = lane
        .accounts
        .estimate(Posture::Platform, PROVIDER, &model, NOW)
        .await
        .expect("answers");
    // The floor's input and output at their own rates, each rounded up on its
    // own, and no cached input and no runtime at issue.
    let up = |tokens: u32, rate: i64| (i64::from(tokens) * rate + PER_MTOK - 1) / PER_MTOK;
    let floor = up(ESTIMATE_FLOOR_INPUT_TOKENS, INPUT_PRICE)
        + up(ESTIMATE_FLOOR_OUTPUT_TOKENS, OUTPUT_PRICE);
    assert_eq!(priced, Estimate::Priced(Nanos::from_i64(floor)));

    let unknown = lane
        .accounts
        .estimate(Posture::Platform, PROVIDER, "not-in-the-catalogue", NOW)
        .await
        .expect("a miss is a value, not an error");
    assert_eq!(unknown, Estimate::Unpriceable);
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_estimate_a_self_managed_run_at_nothing_without_the_catalogue() {
    let lane = Lane::create().await;
    let estimate = lane
        .accounts
        .estimate(Posture::SelfManaged, PROVIDER, "any-model", NOW)
        .await
        .expect("a self-managed estimate needs no catalogue");
    assert_eq!(estimate, Estimate::Priced(Nanos::ZERO));
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
async fn should_snapshot_the_instant_a_wallet_last_ran_dry() {
    let lane = Lane::create().await;
    let exhausted = Timestamp::constant(1_783_000_000, 0);
    lane.wallet(0, Some(exhausted), NOW).await;

    let wallet = lane.billing.snapshot(&lane.tenant).await.expect("answers");
    assert_eq!(wallet.balance.as_i64(), 0);
    assert_eq!(wallet.exhausted_at, Some(exhausted));
    assert_eq!(wallet.updated_at, NOW);
}
