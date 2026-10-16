//! Wallet coverage under integer extremes and concurrently priced turns.
//!
//! Admission multiplies every unresolved obligation by the largest bound; a
//! product past `i64` must refuse, never wrap into an affordable number. Two
//! turns pinned to different prices dispatch at once and each is charged its
//! own rate; their settlements debit one shared wallet without losing either.

use super::{claim, fixture, funded, mint, now, queue};
use afd_authoring::{
    Claimed, ModelCallInput, ModelCalls, ProviderRequest, ProviderResponse, Transport,
};
use afd_core::{id::Uuid7, money::NANOS_PER_USD};
use afd_crypto::{entropy::Entropy, secret::Kek};
use afd_wire::authoring::{
    Accounting, Knowledge, Meter, ModelCall, ModelMessage, ModelRequest, ModelUsage, Outcome,
    Report,
};
use std::sync::Arc;

/// An unresolved obligation whose doubled bound no longer fits an `i64`.
const PAST_HALF: i64 = i64::MAX / 2 + 1;
/// The same obligation shrunk until doubling it fits again.
const BELOW_HALF: i64 = i64::MAX / 4;
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
const OUTSTANDING: &str = "\
INSERT INTO fleet.authoring_attempts(id,lease_id,runner_id,workspace_id,tenant_id,creator_id,
draft_id,message_sequence,fence,deletion_generation,capability_hash,model_snapshot,status,
lease_expires_at,deadline,reserved_nanos,settled_nanos,usage_unknown,calls,input_tokens,
output_tokens,activity_sequence,created_at,updated_at)
VALUES($1::uuid,$1::uuid,$2::uuid,$3::uuid,$4::uuid,$5::uuid,$1::uuid,1,1,0,$1,'{}',
'awaiting_accounting',0,0,$6,0,true,0,0,0,0,$7,$7)";

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn an_obligation_product_past_i64_refuses_admission_instead_of_wrapping() {
    let f = funded(1).await;
    fixture::balance(&f, i64::MAX).await;
    let outstanding = mint();
    sqlx::query(OUTSTANDING)
        .bind(outstanding.as_str())
        .bind(f.runner.as_str())
        .bind(f.workspace.as_str())
        .bind(f.tenant.as_str())
        .bind(f.owner.as_str())
        .bind(PAST_HALF)
        .bind(now().as_millis())
        .execute(&mut *f.connection().await)
        .await
        .expect("an unknown-usage liability that no longer holds a slot");
    let refused = queue(&f, "Two bounds past the integer range").await;
    assert!(claim(&f, &refused).await.is_none(), "overflow never admits");
    assert_eq!(fixture::attempts(&f).await, 1, "only the liability exists");
    let failed = fixture::failure(&f, &refused).await;
    assert_eq!(
        failed
            .failure
            .expect("durable balance failure")
            .recovery_action,
        "owner_billing",
        "the overflow reads as the wallet refusal it is"
    );
    sqlx::query("UPDATE fleet.authoring_attempts SET reserved_nanos=$2 WHERE id=$1::uuid")
        .bind(outstanding.as_str())
        .bind(BELOW_HALF)
        .execute(&mut *f.connection().await)
        .await
        .expect("the liability shrinks below half the range");
    let admitted = queue(&f, "The same wallet once the product fits").await;
    assert!(
        claim(&f, &admitted).await.is_some(),
        "the refusal above was the overflow, not the balance"
    );
    f.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires live Postgres"]
async fn differently_priced_turns_dispatch_and_settle_concurrently_at_their_own_rates() {
    let f = funded(2).await;
    let dear = f.catalogue.get(1).expect("second owned model");
    sqlx::query("UPDATE core.model_library SET input_nanos_per_mtok=$2,output_nanos_per_mtok=$3 WHERE id=$1::uuid")
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
        Ok(ProviderResponse {
            status: 200,
            body: serde_json::to_vec(
                &serde_json::json!({"status":"completed","output":[{"type":"message",
                "content":[{"type":"output_text","text":"private answer"}]}],
                "usage":{"input_tokens":1,"output_tokens":1}}),
            )
            .expect("provider response JSON"),
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
        deletion_generation: lease.deletion_generation.clone(),
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
const AGE_MS: i64 = 1_000;

async fn settle(f: &super::Fixture, lease: &Claimed) -> afd_wire::authoring::Reported {
    sqlx::query("UPDATE fleet.authoring_attempts SET created_at=created_at-$2 WHERE id=$1::uuid")
        .bind(lease.attempt_id.as_str())
        .bind(AGE_MS)
        .execute(&mut *f.connection().await)
        .await
        .expect("a second of elapsed runtime covers the reported millisecond");
    let report = Report::Accounting {
        accounting: Accounting {
            fence: lease.fence.clone(),
            deletion_generation: lease.deletion_generation.clone(),
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
    sqlx::query_scalar("SELECT balance_nanos FROM billing.tenant_wallet WHERE tenant_id=$1::uuid")
        .bind(f.tenant.as_str())
        .fetch_one(&mut *f.connection().await)
        .await
        .expect("owned wallet")
}
