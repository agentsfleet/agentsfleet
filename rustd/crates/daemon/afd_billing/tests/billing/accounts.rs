//! The lease gate's reads, against the rebuilt schema.
//!
//! Each statement is graded on both halves its absence can mean: a workspace
//! with no tenant, a tenant with no wallet, a model the catalogue does not
//! carry. Those are values, never errors, and a reader that turned one into
//! the other would invert a gate.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_billing::rates::Estimate;
use afd_billing::{ESTIMATE_FLOOR_INPUT_TOKENS, ESTIMATE_FLOOR_OUTPUT_TOKENS, Nanos, Posture};
use jiff::Timestamp;

use afd_db::test_util::mint_uuid7;

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
async fn should_resolve_the_payer_and_answer_none_for_an_unknown_workspace() {
    let lane = Lane::create().await;

    let payer = lane.accounts.payer(&lane.workspace).await.expect("answers");
    assert_eq!(payer, Some(lane.tenant.clone()));

    let stranger = lane.accounts.payer(&mint_uuid7()).await.expect("answers");
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
