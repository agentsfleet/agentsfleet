//! Key failures finish chat while real dispatch receipts and wallet holds settle.

mod races;

use super::{Fixture, claim, funded, mint, now, queue};
use afd_authoring::types::{MessageQuery, SendMessage};
use afd_authoring::{Claimed, ModelCalls, ProviderRequest, ProviderResponse, Transport};
use afd_core::{error_code, id::Uuid7};
use afd_crypto::{entropy::Entropy, secret::Kek};
use afd_vault::{SecretBody, SecretName, Vault};
use afd_wire::authoring::{
    Accounting, FailureReason, Knowledge, Meter, ModelCall, ModelMessage, ModelRequest, Outcome,
    Report,
};
use afd_wire::inference::InferenceApi;
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

const KEY: &str = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";
const TEXT: &str = "Check setup with this credential";
const LEDGER: &str = "SELECT a.status,a.usage_unknown,(SELECT count(*) FROM authoring.dispatches WHERE attempt_id=a.id), EXISTS(SELECT 1 FROM billing.holds WHERE work_id=a.id AND released_at IS NULL) FROM authoring.attempts a WHERE id=$1::uuid";

#[derive(Debug)]
struct Provider {
    count: AtomicUsize,
    status: u16,
    body: Value,
}

#[async_trait::async_trait]
impl Transport for Provider {
    async fn send(&self, _request: ProviderRequest) -> afd_authoring::Result<ProviderResponse> {
        self.count.fetch_add(1, Ordering::Relaxed);
        let (headers, body) = crate::provider_wire::body(
            self.status,
            serde_json::to_vec(&self.body).expect("provider JSON"),
        );
        Ok(ProviderResponse {
            status: self.status,
            headers,
            body,
        })
    }
}

async fn replace(f: &Fixture, key: &str) {
    let raw = serde_json::value::RawValue::from_string(
        json!({"provider":"openai","api_key":key}).to_string(),
    )
    .expect("key envelope");
    let body = SecretBody::parse(&raw).expect("sealed key body");
    Vault::new(
        f.db.clone(),
        Arc::new(Kek::from_hex(KEY).expect("fixture key")),
        Entropy::new(),
    )
    .replace(
        &f.workspace,
        &SecretName::parse("authoring-model").expect("key name"),
        &body,
    )
    .await
    .expect("replace credential before admission");
}

async fn call<T: Transport + 'static>(
    f: &Fixture,
    lease: &Claimed,
    provider: Arc<T>,
) -> afd_authoring::Result<afd_authoring::ModelCallResult> {
    let providers = afd_credential::provider::Providers::new(
        f.db.clone(),
        Arc::new(Kek::from_hex(KEY).expect("fixture key")),
        Entropy::new(),
    );
    ModelCalls::new(f.drafts.clone(), providers, provider)
        .call(
            &f.runner,
            &Uuid7::parse(&lease.lease_id).expect("lease"),
            ModelCall {
                capability_token: lease.model_access.token.clone(),
                attempt_id: lease.attempt_id.clone(),
                fence: lease.fence.clone(),
                dispatch_id: mint().to_string(),
                call_index: 1,
                request: ModelRequest {
                    supports_compaction: true,
                    messages: vec![ModelMessage::User { text: TEXT.into() }],
                    max_output_tokens: 64,
                },
            },
        )
        .await
}

async fn state(f: &Fixture, lease: &Claimed) -> (String, bool, i64, bool) {
    sqlx::query_as(LEDGER)
        .bind(&lease.attempt_id)
        .fetch_one(&mut *f.connection().await)
        .await
        .expect("durable accounting state")
}

async fn settled(f: &Fixture, lease: &Claimed, calls: u32) {
    let report = unknown_report(lease, calls);
    let id = Uuid7::parse(&lease.lease_id).expect("lease");
    for replayed in [false, true] {
        let ack = f
            .drafts
            .report(&f.runner, &id, report.clone())
            .await
            .expect("failure settles");
        assert!(ack.settled);
        assert_eq!(ack.state, Outcome::Failed);
        assert_eq!(ack.replayed, replayed);
    }
    assert_eq!(
        state(f, lease).await,
        ("settled".into(), false, i64::from(calls), false)
    );
    let draft = Uuid7::parse(&lease.draft_id).expect("draft");
    let page = f
        .drafts
        .messages(
            f.access(),
            &draft,
            MessageQuery {
                limit: 20,
                starting_after: None,
            },
        )
        .await
        .expect("failure transcript");
    let failure = page
        .items
        .iter()
        .find_map(|message| message.failure.as_ref())
        .expect("visible failure");
    assert_eq!(failure.reason, FailureReason::ModelUnavailable);
    assert_eq!(failure.recovery_action, "choose_authoring_model");
}

fn unknown_report(lease: &Claimed, calls: u32) -> Report {
    Report::Terminal {
        answer: None,
        accounting: Accounting {
            fence: lease.fence.clone(),
            report_id: mint().to_string(),
            outcome: Outcome::Failed,
            failure_reason: Some(FailureReason::ProviderUsageUnknown),
            usage: Meter {
                knowledge: Knowledge::Unknown,
                calls,
                input_tokens: None,
                output_tokens: None,
                runtime_ms: 1,
                provider_cost_nanos: None,
            },
        },
    }
}

// Value: protects=malformed keys never cross durable dispatch or hold unknown usage;
// fails_when=header parsing happens only after intent; seam=physical provider.
#[tokio::test]
#[ignore = "requires live Postgres"]
async fn should_release_unused_hold_when_key_contains_a_literal_newline() {
    Box::pin(malformed_key(InferenceApi::Responses, "fixture-key\n")).await;
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn should_reject_before_intent_when_gemini_key_decodes_to_a_newline() {
    Box::pin(malformed_key(InferenceApi::Gemini, "fixture-key%0A")).await;
}

async fn malformed_key(api: InferenceApi, key: &str) {
    let f = funded(2).await;
    sqlx::query("UPDATE model.platform_models SET inference=jsonb_set(inference,'{api}',$2::jsonb) WHERE id=$1::uuid")
        .bind(f.catalogue[0].as_str())
        .bind(json!(api))
        .execute(&mut *f.connection().await)
        .await
        .expect("select the persisted inference API");
    replace(&f, key).await;
    let draft = queue(&f, TEXT).await;
    let lease = claim(&f, &draft).await.expect("admission");
    assert!(state(&f, &lease).await.3, "reservation is real");
    let provider = Arc::new(Provider {
        count: AtomicUsize::new(0),
        status: 200,
        body: json!({}),
    });
    let fault = call(&f, &lease, Arc::clone(&provider))
        .await
        .expect_err("invalid header key");
    assert_eq!(fault.code(), error_code::AUTHORING_MODEL);
    assert!(!fault.to_string().contains("fixture-key"));
    assert_eq!(provider.count.load(Ordering::Relaxed), 0);
    assert_eq!(state(&f, &lease).await, ("active".into(), false, 0, true));
    settled(&f, &lease, 0).await;
    replace(&f, "repaired-fixture-key").await;
    f.drafts
        .send(
            f.access(),
            &draft.id,
            SendMessage {
                message: TEXT.into(),
                operation_id: mint(),
            },
            now(),
        )
        .await
        .expect("fresh message after repair without Stop");
    assert!(claim(&f, &draft).await.is_some());
    f.cleanup().await;
}

// Value: protects=explicit authentication rejection is known, failed, and charged once;
// fails_when=missing usage on a rejected request retains an unknown hold forever.
#[tokio::test]
#[ignore = "requires live Postgres"]
async fn authentication_rejection_fails_and_settles_without_cancelling_the_chat() {
    let f = funded(2).await;
    let draft = queue(&f, TEXT).await;
    let lease = claim(&f, &draft).await.expect("admission");
    let provider = Arc::new(Provider {
        count: AtomicUsize::new(0),
        status: 401,
        body: json!({"error":{"code":401,"message":"Invalid credentials"}}),
    });
    let fault = call(&f, &lease, Arc::clone(&provider))
        .await
        .expect_err("invalid credentials");
    assert_eq!(fault.code(), error_code::AUTHORING_MODEL);
    assert_eq!(provider.count.load(Ordering::Relaxed), 1);
    assert_eq!(state(&f, &lease).await, ("active".into(), false, 1, true));
    settled(&f, &lease, 1).await;
    let current = f
        .drafts
        .read(f.access(), &draft.id)
        .await
        .expect("chat available");
    assert_eq!(current.cancelled_through, "0");
    f.drafts
        .send(
            f.access(),
            &draft.id,
            SendMessage {
                message: TEXT.into(),
                operation_id: mint(),
            },
            now(),
        )
        .await
        .expect("no unresolved provider work");
    assert!(claim(&f, &draft).await.is_some());
    f.cleanup().await;
}
