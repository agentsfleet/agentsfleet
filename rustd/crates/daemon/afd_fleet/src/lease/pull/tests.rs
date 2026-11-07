//! The verb's first decision, which must be made before any datastore is asked.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the restriction set is for the daemon"
)]

use crate::lease::test_dead;

/// A degraded runner is answered no work without a datastore being asked:
/// the plane here has none that answers, so any read would be a fault.
#[tokio::test]
async fn should_answer_a_degraded_runner_without_touching_a_datastore() {
    let answer = test_dead::plane()
        .lease(&test_dead::id(9), &[], true, test_dead::AT)
        .await
        .expect("a degraded runner is a decision, not a datastore read");

    let body: serde_json::Value = serde_json::from_str(&answer).expect("the answer is JSON");
    assert_eq!(body.get("lease"), Some(&serde_json::Value::Null));
    assert!(
        body.get("retry_after_ms")
            .is_some_and(serde_json::Value::is_u64),
        "the runner is told when to ask again: {body}"
    );
}

/// A failed terminal settlement stays retryable instead of claiming success.
#[tokio::test]
async fn terminal_settlement_propagates_a_database_outage() {
    let refused = test_dead::plane()
        .finished(&test_dead::acquired(), &test_dead::id(9))
        .await
        .err()
        .expect("the durable settlement failed");
    assert!(refused.is_datastore_unavailable(), "{refused}");
}

/// A fleet whose unreadable config cannot even be recorded is answered no
/// work, and neither the config's reason nor the write's failure carries a
/// code.
#[tokio::test]
async fn an_unreadable_config_refusal_reason_carries_no_code_prefix() {
    use afd_core::test_util::trace::Capture;

    let log = Capture::install();
    let fault = crate::Error::from(
        afd_fleet_runtime::FleetConfig::stored("not a document")
            .expect_err("a document that is not JSON does not parse"),
    );

    test_dead::plane()
        .refuse_unreadable_config(&test_dead::acquired(), &test_dead::id(9), &fault)
        .await
        .expect("an unrecorded refusal is logged, not raised");

    let line = log.only(super::refuse::EVENT_CONFIG_REFUSAL_UNRECORDED);
    assert_eq!(line.field("config_reason"), Some(fault.told().as_str()));
    assert_eq!(
        line.field("config_error_code"),
        Some(fault.code().as_str()),
        "the config fault's own code rides beside its reason"
    );
    let reason = line.field("reason").unwrap_or_default();
    assert!(
        !reason.is_empty() && !reason.contains("[UZ-") && !reason.contains('\n'),
        "the write's failure is its sentence alone, its code the line's `error_code`: {reason}"
    );
}
