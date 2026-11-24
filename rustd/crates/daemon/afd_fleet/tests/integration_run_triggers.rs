//! A run adds, lists and removes what may wake its fleet, under its write
//! fence, checked against what the workspace holds.
//!
//! Marked `#[ignore]`; `make test-integration-rustd` runs them. The delivery
//! half — a signed webhook or an App event reaching the fleet — is the
//! ingress suite's.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::borrow::Cow;

use af_core::error_code::{self, ErrorCode};
use af_core::id::Uuid7;
use af_wire::trigger_verb::{TriggerAddRequest, TriggerKind, TriggerVerifier};
use afd_fleet::run::standing::Standing;
use afd_fleet::run::write_fence::WriteFence;
use afd_setup::trigger::TriggerRow;
use serde_json::json;

use crate::report_seed::{Held, held};

/// The vault entry a fixture webhook is signed with.
const SIGNING_SECRET: &str = "sentry-hook";

/// A Sentry webhook signed by [`SIGNING_SECRET`].
fn sentry_webhook(source: &'static str) -> TriggerAddRequest<'static> {
    TriggerAddRequest {
        fencing_token: 0,
        item_id: Cow::Owned(afd_db::test_util::mint_id().to_string()),
        call_number: 1,
        kind: TriggerKind::Webhook,
        source: Some(Cow::Borrowed(source)),
        events: None,
        repositories: None,
        verifier: Some(TriggerVerifier {
            scheme: Cow::Borrowed("svix-v1"),
            header: None,
        }),
        secret_name: Some(Cow::Borrowed(SIGNING_SECRET)),
        channel: None,
    }
}

/// An add of `kind` with no source, which takes its kind's one source.
fn provider_trigger(kind: TriggerKind) -> TriggerAddRequest<'static> {
    TriggerAddRequest {
        kind,
        source: None,
        verifier: None,
        secret_name: None,
        channel: (kind == TriggerKind::Mention).then_some(Cow::Borrowed("C0123456789")),
        repositories: (kind == TriggerKind::AppEvent).then(|| vec![Cow::Borrowed("org/app")]),
        ..sentry_webhook("sentry")
    }
}

/// The held run's standing and its write fence.
async fn fenced(held: &Held) -> (Standing, WriteFence) {
    let fence = held.fence.as_u64();
    let standing = held
        .fixtures
        .plane()
        .standing(&held.runner, held.run_key.clone(), fence)
        .await
        .expect("the held run stands");
    let guard = WriteFence::new(held.runner.clone(), held.run_key.clone(), fence);
    (standing, guard)
}

/// Adds `request` as the held run.
async fn add(held: &Held, request: &TriggerAddRequest<'_>) -> afd_fleet::Result<TriggerRow> {
    let (standing, guard) = fenced(held).await;
    let item = Uuid7::parse(&request.item_id).expect("a minted item");
    held.fixtures
        .plane()
        .add_trigger(&standing, &item, request, &guard)
        .await
}

/// The held fleet's trigger rows.
async fn rows(held: &Held) -> Vec<TriggerRow> {
    let (standing, _guard) = fenced(held).await;
    held.fixtures
        .plane()
        .triggers(&standing)
        .await
        .expect("the fleet's triggers read")
}

/// Stores the signing secret in the held fleet's workspace.
async fn store_signing_secret(held: &Held) {
    held.fixtures
        .store_secret(
            &held.acquired.workspace_id,
            SIGNING_SECRET,
            &json!({"secret": "whsec_fixture"}),
        )
        .await;
}

fn code(answer: afd_fleet::Result<TriggerRow>) -> Option<ErrorCode> {
    answer.err().map(|refusal| refusal.code())
}

// Value: protects=a webhook needs its signing secret in the workspace's vault,
// and may not take a reserved source; fails_when=a row is written for a
// webhook no delivery could be verified against; seam=`RunService::add_trigger`
// over live Postgres.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_webhook_trigger_needs_a_stored_secret() {
    let held = held().await;

    let unsigned = add(&held, &sentry_webhook("sentry")).await;
    store_signing_secret(&held).await;
    let reserved = add(&held, &sentry_webhook("github")).await;

    assert_eq!(
        code(unsigned),
        Some(error_code::FLEET_TRIGGER_SECRET_NOT_FOUND)
    );
    assert_eq!(code(reserved), Some(error_code::INVALID_REQUEST));
    assert_eq!(rows(&held).await.len(), 0, "no row");
    held.fixtures.cleanup().await;
}

// Value: protects=an App event or a mention needs its provider connected to
// the workspace; fails_when=a row is written for a provider that can deliver
// nothing; seam=`RunService::add_trigger` over live Postgres.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_trigger_for_an_unconnected_provider_is_refused() {
    let held = held().await;

    let github = add(&held, &provider_trigger(TriggerKind::AppEvent)).await;
    let slack = add(&held, &provider_trigger(TriggerKind::Mention)).await;

    assert_eq!(code(github), Some(error_code::FLEET_PROVIDER_NOT_CONNECTED));
    assert_eq!(code(slack), Some(error_code::FLEET_PROVIDER_NOT_CONNECTED));
    assert_eq!(rows(&held).await.len(), 0, "no row");
    held.fixtures.cleanup().await;
}

// Value: protects=a source takes one webhook row, and a retried add answers
// the row it wrote; fails_when=a second add for one source writes a second
// row, or a retry is refused; seam=`RunService::add_trigger` over live Postgres.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_second_webhook_for_one_source_is_refused() {
    let held = held().await;
    store_signing_secret(&held).await;
    let first = sentry_webhook("sentry");
    let written = add(&held, &first)
        .await
        .expect("the first webhook is added");

    let retried = add(&held, &first).await.expect("a retry answers its row");
    let second = add(&held, &sentry_webhook("sentry")).await;

    assert_eq!(retried.id, written.id);
    assert_eq!(code(second), Some(error_code::FLEET_TRIGGER_EXISTS));
    assert_eq!(rows(&held).await.len(), 1);
    held.fixtures.cleanup().await;
}

// Value: protects=a removed trigger is gone from the fleet's list, and a second
// remove finds nothing; fails_when=the remove leaves the row or answers it
// removed twice; seam=`RunService::remove_trigger` over live Postgres;
// no_claim=the delivery refused at the removed source, which ingress answers.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_removed_trigger_admits_nothing() {
    let held = held().await;
    store_signing_secret(&held).await;
    let row = add(&held, &sentry_webhook("sentry"))
        .await
        .expect("the webhook is added");
    let (standing, guard) = fenced(&held).await;
    let plane = held.fixtures.plane();

    let removed = plane
        .remove_trigger(&standing, &row.id, &guard)
        .await
        .expect("the remove answers");
    let again = plane
        .remove_trigger(&standing, &row.id, &guard)
        .await
        .expect("a second remove answers");

    assert!(removed);
    assert!(!again, "nothing is left to remove");
    assert_eq!(rows(&held).await.len(), 0);
    held.fixtures.cleanup().await;
}
