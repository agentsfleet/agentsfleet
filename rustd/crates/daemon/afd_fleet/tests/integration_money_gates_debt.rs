//! The balance gate against a tenant in debt.
//!
//! A settlement takes its whole charge, so a balance can sit below zero; the
//! gate reads the balance less every unsettled line, and a debt must refuse.
//! Marked `#[ignore]` like the rest of the live-service suite; run by
//! `make test-integration-rustd`.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_billing::Accounts;
use afd_fleet::lease::admit::{Admission, money_gates};

use afd_db::test_util::mint_id;

use crate::integration_money_gates::{AMPLE_DAILY_NANOS, biller, budgeted, request};
use crate::seed::unique_ids;
use crate::support::Fixtures;

/// What a tenant in debt owes, and then holds once it is repaid.
const DEBT_NANOS: i64 = 50;

/// A tenant in debt is refused at the balance gate, and admitted once its
/// balance is back above zero.
///
/// A settlement takes its whole charge, so a balance can sit below zero. The
/// model is unpriced, so the floor is zero: what refuses here is the debt
/// alone, and an admission gate that clamped the balance at zero would cover a
/// zero floor and admit a payer who owes money.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_tenant_in_debt_is_refused_until_its_balance_is_repaid() {
    let fixtures = Fixtures::create().await;
    let accounts = Accounts::new(fixtures.database.clone());
    let biller = biller(&fixtures);
    let (fleet, workspace, tenant) = unique_ids();
    fixtures
        .seed_fleet(&fleet, &workspace, &tenant, "money-gates")
        .await;
    fixtures.seed_wallet(&tenant, -DEBT_NANOS).await;
    let config = budgeted(AMPLE_DAILY_NANOS);
    let (in_debt, repaid) = (mint_id(), mint_id());

    let decided = money_gates(
        &accounts,
        &biller,
        accounts.payer(&workspace).await,
        request(&workspace, &fleet, &in_debt, &config),
        fixtures.enrolled_at(),
    )
    .await
    .expect("a debt is a decision, not a fault");
    let Admission::Refuse(refusal) = decided else {
        panic!("a tenant in debt must be refused, got {decided:?}");
    };
    assert_eq!(refusal.label, af_core::event::label::BALANCE_EXHAUSTED);

    fixtures.seed_wallet(&tenant, DEBT_NANOS).await;
    let decided = money_gates(
        &accounts,
        &biller,
        accounts.payer(&workspace).await,
        request(&workspace, &fleet, &repaid, &config),
        fixtures.enrolled_at(),
    )
    .await
    .expect("an admitted run is not a fault");
    assert!(
        matches!(decided, Admission::Admit(_)),
        "a repaid balance admits: {decided:?}"
    );

    fixtures.cleanup().await;
}
