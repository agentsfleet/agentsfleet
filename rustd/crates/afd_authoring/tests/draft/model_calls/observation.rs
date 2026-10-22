//! Durable observations carry bounded failure facts and respect explicit Stop.

use super::{
    super::{attempt::Attempt, fixture::Fixture},
    fixture::{self, Provider},
};
use afd_authoring::{Observation, types::Source};
use afd_core::error_code;
use afd_wire::authoring::{Accounting, Knowledge, Meter, Outcome, Report};
use serde_json::{Value, json};
use std::sync::Arc;

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn uncertain_failure_notice_preserves_cause_and_stop_changes_only_recovery() {
    let f = Fixture::create().await;
    let attempt = Attempt::start(&f, f.access(), Source::Blank).await;
    let mut provider = Provider::known();
    provider.response["usage"] = Value::Null;
    let provider = Arc::new(provider);
    let calls = fixture::calls(&f, &attempt, Arc::clone(&provider)).await;
    let request = fixture::request(&attempt);
    let error = calls
        .call(&f.runner, &attempt.lease, request.clone())
        .await
        .expect_err("missing authoritative usage");
    assert_eq!(error.code(), error_code::AUTHORING_UNKNOWN);
    assert_eq!(
        calls
            .call(&f.runner, &attempt.lease, request.clone())
            .await
            .expect_err("unsettled usage cannot redispatch")
            .code(),
        error_code::AUTHORING_UNKNOWN
    );
    let acknowledged = f
        .drafts
        .report(&f.runner, &attempt.lease, report())
        .await
        .expect("unknown report acknowledgement");
    assert!(!acknowledged.settled);
    assert_failure(&f, &attempt, "await_accounting").await;
    f.drafts
        .cancel(f.access(), &attempt.draft.id, 1)
        .await
        .expect("explicit Stop");
    assert_failure(&f, &attempt, "retry_message").await;
    assert_eq!(
        calls
            .call(&f.runner, &attempt.lease, request)
            .await
            .expect_err("Stop never redispatches uncertain work")
            .code(),
        error_code::AUTHORING_FENCE
    );
    assert_eq!(provider.count(), 1);
    assert_eq!(fixture::receipts(&f, &attempt).await, (1, 1, true));
    f.cleanup().await;
}

fn report() -> Report {
    Report::Terminal {
        accounting: Accounting {
            fence: "1".into(),
            report_id: super::super::mint().to_string(),
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
        answer: Some("private answer that unknown usage cannot publish".into()),
    }
}

async fn assert_failure(f: &Fixture, attempt: &Attempt, action: &str) {
    let observed = f
        .drafts
        .observe(f.access(), &attempt.draft.id, 0)
        .await
        .expect("authorized durable observation");
    let Observation::Current(view) = observed else {
        panic!("live draft was deleted");
    };
    let message = view
        .notices
        .iter()
        .find(|notice| notice.sequence.as_deref() == Some("1"))
        .expect("message notice");
    assert_eq!(
        serde_json::to_value(message.failure.as_ref().expect("durable failure"))
            .expect("safe failure JSON"),
        json!({"reason":"provider_usage_unknown",
            "error_code":error_code::AUTHORING_UNKNOWN.as_str(),"recovery_action":action,
            "cause":"completion_unverified"})
    );
    assert!(
        !serde_json::to_string(message)
            .expect("notice JSON")
            .contains("private answer")
    );
}
