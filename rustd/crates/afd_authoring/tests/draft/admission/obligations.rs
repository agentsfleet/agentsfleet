//! Admission against the biller's holds, and concurrently priced turns.
//!
//! Admission reads the balance less every open hold; a hold that leaves less
//! than the attempt's bound refuses it as the wallet refusal it is. Two turns
//! pinned to different prices dispatch at once and each is charged its own
//! rate; their settlements debit one shared wallet without losing either.

use super::{claim, fixture, funded, mint, now, queue};
use afd_authoring::{
    Claimed, ModelCallInput, ModelCalls, ProviderRequest, ProviderResponse, Transport,
};
use afd_billing::Nanos;
use afd_billing::biller::{Account, Release, Reservation, WorkKey};
use afd_core::{id::Uuid7, money::NANOS_PER_USD};
use afd_crypto::{entropy::Entropy, secret::Kek};
use afd_wire::authoring::{
    Accounting, Knowledge, Meter, ModelCall, ModelMessage, ModelRequest, ModelUsage, Outcome,
    Report,
};
use std::sync::Arc;

/// Three times the fixture model's rates: three and six dollars per million.
const DEAR_INPUT: i64 = 3 * NANOS_PER_USD;
/// See [`DEAR_INPUT`].
const DEAR_OUTPUT: i64 = 6 * NANOS_PER_USD;
/// One input and one output token at the fixture rates, rounded up per class.
const CHEAP_COST: &str = "3000";
/// One input and one output token at [`DEAR_INPUT`] and [`DEAR_OUTPUT`].
const DEAR_COST: &str = "9000";
/// One reported millisecond of runtime at the runtime rate.
const RUNTIME_DEBIT: i64 = 100;
/// What a wallet in debt owes.
const DEBT: i64 = 50;

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn an_open_hold_that_leaves_less_than_the_bound_refuses_admission() {
    let f = funded(1).await;
    fixture::balance(&f, NANOS_PER_USD).await;
    let biller = crate::draft::fixture::ledger(&f.db);
    let elsewhere = Reservation {
        work: WorkKey::AuthoringAttempt(mint()),
        payer: Account {
            tenant: f.tenant.clone(),
            workspace: f.workspace.clone(),
        },
        ceiling: Nanos::from_i64(NANOS_PER_USD),
    };
    assert!(
        biller
            .reserve(&elsewhere)
            .await
            .expect("the biller answers")
            .admits(),
        "another setup chat holds the whole balance"
    );
    let refused = queue(&f, "A turn the held balance cannot fund").await;
    assert!(
        claim(&f, &refused).await.is_none(),
        "a held balance never admits"
    );
    assert_eq!(fixture::attempts(&f).await, 0, "no attempt was issued");
    let failed = fixture::failure(&f, &refused).await;
    assert_eq!(
        failed
            .failure
            .expect("durable balance failure")
            .recovery_action,
        "owner_billing",
        "the held balance reads as the wallet refusal it is"
    );
    biller
        .release(&elsewhere.work, Release::Completed)
        .await
        .expect("the other hold releases");
    let admitted = queue(&f, "The same wallet once the hold is gone").await;
    assert!(
        claim(&f, &admitted).await.is_some(),
        "the refusal above was the hold, not the balance"
    );
    f.cleanup().await;
}

/// A wallet in debt refuses the claim as the wallet refusal, and admits once a
/// grant repays the debt. A settlement takes its whole charge, so the balance
/// the gate reads can sit below zero.
#[tokio::test]
#[ignore = "requires live Postgres"]
async fn a_wallet_in_debt_refuses_admission_until_it_is_repaid() {
    let f = funded(1).await;
    fixture::balance(&f, -DEBT).await;
    let refused = queue(&f, "A turn a wallet in debt cannot fund").await;
    assert!(claim(&f, &refused).await.is_none(), "a debt never admits");
    assert_eq!(fixture::attempts(&f).await, 0, "no attempt was issued");
    let failed = fixture::failure(&f, &refused).await;
    assert_eq!(
        failed
            .failure
            .expect("durable debt failure")
            .recovery_action,
        "owner_billing",
        "a debt reads as the wallet refusal"
    );
    fixture::balance(&f, NANOS_PER_USD).await;
    let admitted = queue(&f, "The same wallet once the debt is repaid").await;
    assert!(
        claim(&f, &admitted).await.is_some(),
        "a repaid wallet admits"
    );
    f.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires live Postgres"]
async fn differently_priced_turns_dispatch_and_settle_concurrently_at_their_own_rates() {
    let f = funded(2).await;
    let dear = f.catalogue.get(1).expect("second owned model");
    sqlx::query("UPDATE model.platform_model_prices SET input_nanos_per_mtok=$2,output_nanos_per_mtok=$3 WHERE platform_model_id=$1::uuid")
        .bind(dear.as_str()).bind(DEAR_INPUT).bind(DEAR_OUTPUT)
        .execute(&mut *f.connection().await).await.expect("the second model costs three times as much");
    let cheap = queue(&f, "Cheap turn").await;
    let mut request = f.request(mint(), Some("Dear turn"));
    request.authoring_model_ref = f.other_model.to_string();
    let dearer = f
        .drafts
        .create(f.access(), request, now())
        .await
        .expect("queue a turn on the dearer model")
        .draft;
    let (cheap_lease, dear_lease) = tokio::join!(claim(&f, &cheap), claim(&f, &dearer));
    let (cheap_lease, dear_lease) = (
        cheap_lease.expect("cheap turn admitted"),
        dear_lease.expect("dear turn admitted"),
    );
    let calls = model_calls(&f);
    let (first, second) = tokio::join!(
        dispatch(&f, &calls, &cheap_lease),
        dispatch(&f, &calls, &dear_lease)
    );
    assert_eq!(
        cost(&first),
        Some(CHEAP_COST),
        "the cheap turn pays its own rate"
    );
    assert_eq!(
        cost(&second),
        Some(DEAR_COST),
        "the dear turn pays its own rate"
    );
    let before = fixture_balance(&f).await;
    let (cheap_report, dear_report) =
        tokio::join!(settle(&f, &cheap_lease), settle(&f, &dear_lease));
    assert!(cheap_report.settled && dear_report.settled);
    assert!(!cheap_report.replayed && !dear_report.replayed);
    assert_eq!(
        fixture_balance(&f).await,
        before - 2 * RUNTIME_DEBIT,
        "both runtime debits land under the wallet lock; neither is lost"
    );
    f.cleanup().await;
}

/// The provider is the only fake: one input token in, one output token out.
#[derive(Debug)]
struct OneTokenProvider;

#[async_trait::async_trait]
impl Transport for OneTokenProvider {
    async fn send(&self, _request: ProviderRequest) -> afd_authoring::Result<ProviderResponse> {
        let (headers, body) = crate::provider_wire::body(
            200,
            serde_json::to_vec(
                &serde_json::json!({"status":"completed","output":[{"type":"message",
                "content":[{"type":"output_text","text":"private answer"}]}],
                "usage":{"input_tokens":1,"output_tokens":1}}),
            )
            .expect("provider response JSON"),
        );
        Ok(ProviderResponse {
            status: 200,
            headers,
            body,
        })
    }
}

fn model_calls(f: &super::Fixture) -> ModelCalls {
    let kek = Kek::from_hex("00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff")
        .expect("the key the fixture credential was sealed under");
    let providers =
        afd_credential::provider::Providers::new(f.db.clone(), Arc::new(kek), Entropy::new());
    ModelCalls::new(f.drafts.clone(), providers, Arc::new(OneTokenProvider))
}

async fn dispatch(f: &super::Fixture, calls: &ModelCalls, lease: &Claimed) -> ModelUsage {
    let input: ModelCallInput = ModelCall {
        capability_token: lease.model_access.token.clone(),
        attempt_id: lease.attempt_id.clone(),
        fence: lease.fence.clone(),
        dispatch_id: mint().to_string(),
        call_index: 1,
        request: ModelRequest {
            messages: vec![ModelMessage::User {
                text: "Investigate the checkout".into(),
            }],
            max_output_tokens: 64,
        },
    };
    calls
        .call(&f.runner, &id(&lease.lease_id), input)
        .await
        .expect("an admitted, pinned dispatch")
        .usage
}

/// How far a turn is backdated so its attempt reads as already started.
const AGE: std::time::Duration = std::time::Duration::from_secs(1);

async fn settle(f: &super::Fixture, lease: &Claimed) -> afd_wire::authoring::Reported {
    sqlx::query(
        "UPDATE authoring.attempts SET created_at=created_at-$2::interval WHERE id=$1::uuid",
    )
    .bind(lease.attempt_id.as_str())
    .bind(AGE)
    .execute(&mut *f.connection().await)
    .await
    .expect("a second of elapsed runtime covers the reported millisecond");
    let report = Report::Accounting {
        accounting: Accounting {
            fence: lease.fence.clone(),
            report_id: mint().to_string(),
            outcome: Outcome::Completed,
            failure_reason: None,
            usage: Meter {
                knowledge: Knowledge::Known,
                calls: 0,
                input_tokens: Some(0),
                output_tokens: Some(0),
                runtime_ms: 1,
                provider_cost_nanos: None,
            },
        },
    };
    f.drafts
        .report(&f.runner, &id(&lease.lease_id), report)
        .await
        .expect("the report settles")
}

/// The receipt's decimal cost; `None` when the reply left usage unknown.
fn cost(usage: &ModelUsage) -> Option<&str> {
    match usage {
        ModelUsage::Known {
            provider_cost_nanos,
            ..
        } => Some(provider_cost_nanos),
        ModelUsage::Unknown => None,
    }
}

fn id(raw: &str) -> Uuid7 {
    Uuid7::parse(raw).expect("the lease names a UUIDv7")
}

async fn fixture_balance(f: &super::Fixture) -> i64 {
    sqlx::query_scalar("SELECT balance_nanos FROM billing.wallets WHERE tenant_id=$1::uuid")
        .bind(f.tenant.as_str())
        .fetch_one(&mut *f.connection().await)
        .await
        .expect("owned wallet")
}
