//! Retained provider liability must not regain private chat ownership after Stop.

use super::{Fixture, claim, funded, mint, now, queue};
use afd_authoring::types::{Draft, Failure, MessageQuery, SendMessage};
use afd_authoring::{Claimed, ModelCalls, ProviderRequest, ProviderResponse, Transport};
use afd_core::{error_code, id::Uuid7};
use afd_credential::provider::Providers;
use afd_crypto::{entropy::Entropy, secret::Kek};
use afd_wire::authoring::{
    Accounting, ActivityEvent, ActivityRequest, FailureReason, Knowledge, Meter, ModelCall,
    ModelMessage, ModelRequest, Outcome, Proposal, Report,
};
use std::sync::Arc;

const FIRST: &str = "A request whose provider usage cannot be read";
const NEXT: &str = "Continue after stopping the old turn";
const ACTION_RETRY_MESSAGE: &str = "retry_message";
const ACTION_AWAIT_ACCOUNTING: &str = "await_accounting";
const ATTEMPTS: &str =
    "SELECT count(*),bool_or(usage_unknown) FROM authoring.attempts WHERE draft_id=$1::uuid";

#[test]
fn failure_recovery_actions_preserve_the_daemons_reason_and_code() {
    for (reason, code, action) in [
        (
            FailureReason::InsufficientBalance,
            error_code::AUTHORING_BALANCE,
            "owner_billing",
        ),
        (
            FailureReason::ModelUnavailable,
            error_code::AUTHORING_MODEL,
            "choose_authoring_model",
        ),
        (
            FailureReason::ContextTooLarge,
            error_code::AUTHORING_TOKEN,
            "reduce_context",
        ),
        (
            FailureReason::ProviderFailed,
            error_code::AUTHORING_PROVIDER,
            ACTION_RETRY_MESSAGE,
        ),
        (
            FailureReason::ProviderUsageUnknown,
            error_code::AUTHORING_UNKNOWN,
            ACTION_AWAIT_ACCOUNTING,
        ),
        (
            FailureReason::AttemptTimedOut,
            error_code::AUTHORING_TIMEOUT,
            ACTION_RETRY_MESSAGE,
        ),
        (
            FailureReason::PolicyChanged,
            error_code::AUTHORING_POLICY,
            "review_setup",
        ),
    ] {
        let failure = Failure::from(reason);
        assert_eq!(failure.reason, reason);
        assert_eq!(failure.error_code, code);
        assert_eq!(failure.recovery_action, action);
    }
}

#[derive(Debug)]
struct UnknownProvider;

#[async_trait::async_trait]
impl Transport for UnknownProvider {
    async fn send(&self, _request: ProviderRequest) -> afd_authoring::Result<ProviderResponse> {
        let (headers, body) =
            crate::provider_wire::body(503, br#"{"error":{"message":"unavailable"}}"#.to_vec());
        Ok(ProviderResponse {
            status: 503,
            headers,
            body,
        })
    }
}

pub(super) async fn unknown(f: &Fixture, lease: &Claimed) {
    let kek =
        Kek::from_hex(&"00112233445566778899aabbccddeeff".repeat(2)).expect("fixture vault key");
    let providers = Providers::new(f.db.clone(), Arc::new(kek), Entropy::new());
    let calls = ModelCalls::new(f.drafts.clone(), providers, Arc::new(UnknownProvider));
    let token = lease.model_access.token.clone();
    let input = ModelCall {
        capability_token: token,
        attempt_id: lease.attempt_id.clone(),
        fence: lease.fence.clone(),
        dispatch_id: mint().to_string(),
        call_index: 1,
        request: ModelRequest {
            supports_compaction: true,
            messages: vec![ModelMessage::User { text: FIRST.into() }],
            max_output_tokens: 64,
        },
    };
    assert_eq!(
        calls
            .call(
                &f.runner,
                &Uuid7::parse(&lease.lease_id).expect("lease"),
                input
            )
            .await
            .expect_err("missing authoritative usage")
            .code(),
        error_code::AUTHORING_UNKNOWN
    );
    let report = unknown_report(lease);
    let id = Uuid7::parse(&lease.lease_id).expect("lease");
    let ack = f
        .drafts
        .report(&f.runner, &id, report)
        .await
        .expect("runner's retained unknown report");
    assert!(!ack.settled);
}

fn unknown_report(lease: &Claimed) -> Report {
    Report::Terminal {
        answer: None,
        accounting: Accounting {
            fence: lease.fence.clone(),
            report_id: mint().to_string(),
            outcome: Outcome::Failed,
            failure_reason: Some(FailureReason::ProviderUsageUnknown),
            usage: Meter {
                knowledge: Knowledge::Unknown,
                calls: 1,
                input_tokens: None,
                output_tokens: None,
                runtime_ms: 0,
                provider_cost_nanos: None,
            },
        },
    }
}

async fn failed(f: &Fixture, draft: &Draft) -> afd_authoring::types::Failure {
    f.drafts
        .messages(
            f.access(),
            &draft.id,
            MessageQuery {
                limit: 20,
                starting_after: None,
            },
        )
        .await
        .expect("private transcript")
        .items
        .into_iter()
        .find_map(|message| message.failure)
        .expect("durable provider failure")
}

// Value: protects=unknown provider work refuses fresh text but retains accepted replay;
// fails_when=Send queues behind an unresolved turn or consumes its sequence;
// why_new=the old Send accepted text which claim could never run; seam=physical provider only.
#[tokio::test]
#[ignore = "requires live Postgres"]
async fn unknown_work_refuses_fresh_send_but_preserves_exact_replay() {
    let f = funded(2).await;
    let operation = mint();
    let draft = f
        .drafts
        .create(f.access(), f.request(operation.clone(), Some(FIRST)), now())
        .await
        .expect("accepted first request")
        .draft;
    let lease = claim(&f, &draft).await.expect("first attempt");
    unknown(&f, &lease).await;
    let original = SendMessage {
        save: false,
        message: FIRST.into(),
        operation_id: operation,
    };
    let replay = f
        .drafts
        .send(f.access(), &draft.id, original, now())
        .await
        .expect("exact accepted replay remains available");
    assert!(replay.replayed);
    assert_eq!(replay.sequence, "1");
    let input = SendMessage {
        save: false,
        message: NEXT.into(),
        operation_id: mint(),
    };
    let error = f
        .drafts
        .send(f.access(), &draft.id, input, now())
        .await
        .expect_err("new text cannot wait indefinitely behind unknown work");
    assert_eq!(error.code(), error_code::AUTHORING_UNKNOWN);
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM authoring.messages WHERE draft_id=$1::uuid")
            .bind(draft.id.as_str())
            .fetch_one(&mut *f.connection().await)
            .await
            .expect("message count");
    assert_eq!(count, 1);
    assert_eq!(
        failed(&f, &draft).await.recovery_action,
        ACTION_AWAIT_ACCOUNTING
    );
    f.cleanup().await;
}

// Value: protects=Stop releases chat ownership while old liability remains;
// fails_when=claim still treats a stopped attempt as the current turn;
// why_new=both discovery and locked admission ignored the watermark; seam=physical provider only.
#[tokio::test]
#[ignore = "requires live Postgres"]
async fn stopped_unknown_work_does_not_block_new_claim_or_publish_late_text() {
    let f = funded(2).await;
    let draft = queue(&f, FIRST).await;
    let lease = claim(&f, &draft).await.expect("first attempt");
    unknown(&f, &lease).await;
    f.drafts
        .cancel(f.access(), &draft.id, 1)
        .await
        .expect("Stop unknown work");
    let sent = f
        .drafts
        .send(
            f.access(),
            &draft.id,
            SendMessage {
                save: false,
                message: NEXT.into(),
                operation_id: mint(),
            },
            now(),
        )
        .await
        .expect("fresh request after Stop");
    assert_eq!(sent.sequence, "2");
    let newer = claim(&f, &draft)
        .await
        .expect("stopped liability cannot own current chat");
    assert_ne!(newer.attempt_id, lease.attempt_id);
    assert_eq!(newer.sequence, "2");
    let retained: (i64, bool) = sqlx::query_as(ATTEMPTS)
        .bind(draft.id.as_str())
        .fetch_one(&mut *f.connection().await)
        .await
        .expect("old obligation retained");
    assert_eq!(retained, (2, true));
    assert_eq!(
        failed(&f, &draft).await.recovery_action,
        ACTION_RETRY_MESSAGE
    );
    late_cannot_publish(&f, &lease).await;
    f.drafts
        .cancel(f.access(), &draft.id, 1)
        .await
        .expect("delayed old Stop");
    let status: String = sqlx::query_scalar(
        "SELECT status FROM authoring.messages WHERE draft_id=$1::uuid AND sequence=2",
    )
    .bind(draft.id.as_str())
    .fetch_one(&mut *f.connection().await)
    .await
    .expect("new turn state");
    assert_eq!(status, "running");
    f.cleanup().await;
}

async fn late_cannot_publish(f: &Fixture, lease: &Claimed) {
    let id = Uuid7::parse(&lease.lease_id).expect("old lease");
    let proposal = ActivityRequest {
        fence: lease.fence.clone(),
        sequence: lease.sequence.clone(),
        activity_sequence: "1".into(),
        event: ActivityEvent::Proposal {
            proposal: Proposal::Initialize {
                definition: Box::new(super::super::definition()),
            },
        },
    };
    let error = f
        .drafts
        .activity(&f.runner, &id, proposal)
        .await
        .expect_err("old proposal fenced");
    assert_eq!(error.code(), error_code::AUTHORING_FENCE);
    let before = f.counts().await;
    let ack = f
        .drafts
        .report(&f.runner, &id, super::stop::finished(lease))
        .await
        .expect("retained late report");
    assert!(!ack.settled, "uncertain usage is never invented");
    assert_eq!(
        f.counts().await,
        before,
        "old answer and proposal cannot append"
    );
}
