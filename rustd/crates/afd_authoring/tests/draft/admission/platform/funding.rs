//! Real platform admission must fund each measured send before provider access.
use super::{Fixture, claim, configured, queued};
use afd_authoring::{
    Claimed, ModelCallInput, ModelCalls, ProviderRequest, ProviderResponse, Transport,
};
use afd_core::{error_code, id::Uuid7};
use afd_crypto::{entropy::Entropy, secret::Kek};
use afd_wire::authoring::{ModelCall, ModelMessage, ModelRequest};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

const ADMISSION: i64 = 46_384_007;

#[derive(Debug, Default)]
struct Provider(AtomicUsize);
#[async_trait::async_trait]
impl Transport for Provider {
    async fn send(&self, _request: ProviderRequest) -> afd_authoring::Result<ProviderResponse> {
        self.0.fetch_add(1, Ordering::SeqCst);
        let response = serde_json::json!({"status":"completed","output":[{"type":"message",
            "content":[{"type":"output_text","text":"funded answer"}]}],
            "usage":{"input_tokens":100,"output_tokens":1}});
        let (headers, body) =
            crate::provider_wire::body(200, serde_json::to_vec(&response).expect("wire reply"));
        Ok(ProviderResponse {
            status: 200,
            headers,
            body,
        })
    }
}

fn calls(f: &Fixture, provider: Arc<Provider>) -> ModelCalls {
    let key = Kek::from_hex("00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff")
        .expect("fixture key");
    let providers =
        afd_credential::provider::Providers::new(f.db.clone(), Arc::new(key), Entropy::new());
    ModelCalls::new(f.drafts.clone(), providers, provider)
}

fn request(lease: &Claimed, index: u32) -> ModelCallInput {
    ModelCall {
        capability_token: lease.model_access.token.clone(),
        attempt_id: lease.attempt_id.clone(),
        fence: lease.fence.clone(),
        dispatch_id: super::mint().to_string(),
        call_index: index,
        request: ModelRequest {
            supports_compaction: true,
            messages: vec![ModelMessage::User {
                text: "Use the configured platform model".into(),
            }],
            max_output_tokens: 64,
        },
    }
}

async fn durable(f: &Fixture, lease: &Claimed) -> (i64, i64, i64) {
    sqlx::query_as("SELECT a.reserved_nanos,h.ceiling_nanos,(SELECT count(*) FROM authoring.dispatches d WHERE d.attempt_id=a.id) FROM authoring.attempts a JOIN billing.holds h ON h.work_id=a.id WHERE a.id=$1::uuid")
        .bind(&lease.attempt_id).fetch_one(&mut *f.connection().await).await.expect("owned reservation and receipts")
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn each_platform_send_funds_measured_input_and_keeps_prior_known_cost() {
    let f = configured().await;
    let draft = queued(&f).await;
    let lease = claim(&f, &draft).await.expect("platform admission");
    let id = Uuid7::parse(&lease.lease_id).expect("lease");
    let provider = Arc::new(Provider::default());
    let calls = calls(&f, Arc::clone(&provider));
    assert_eq!(durable(&f, &lease).await, (ADMISSION, ADMISSION, 0));
    let mut previous = ADMISSION;
    for index in 1..=2 {
        let reply = calls
            .call(&f.runner, &id, request(&lease, index))
            .await
            .expect("funded physical call");
        assert_eq!(reply.text, "funded answer");
        let (reserved, held, receipts) = durable(&f, &lease).await;
        assert!(
            reserved > previous,
            "each send must cover measured input and known prior spend"
        );
        assert_eq!(held, reserved);
        assert_eq!(receipts, i64::from(index));
        previous = reserved;
    }
    assert_eq!(provider.0.load(Ordering::SeqCst), 2);
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn an_admission_only_balance_refuses_measured_input_before_any_dispatch() {
    let f = configured().await;
    let draft = queued(&f).await;
    let lease = claim(&f, &draft).await.expect("platform admission");
    super::super::fixture::balance(&f, ADMISSION).await;
    let provider = Arc::new(Provider::default());
    let calls = calls(&f, Arc::clone(&provider));
    let error = calls
        .call(
            &f.runner,
            &Uuid7::parse(&lease.lease_id).expect("lease"),
            request(&lease, 1),
        )
        .await
        .expect_err("input needs additional funding");
    assert_eq!(error.code(), error_code::AUTHORING_BALANCE);
    assert_eq!(provider.0.load(Ordering::SeqCst), 0);
    assert_eq!(durable(&f, &lease).await, (ADMISSION, ADMISSION, 0));
    f.cleanup().await;
}
