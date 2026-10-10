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

/// A finished event whose acknowledgement the queue refused is still answered
/// no work, and the warning's reason is the failure's sentence alone.
#[tokio::test]
async fn an_unacknowledged_terminal_reason_carries_no_code_prefix() {
    use afd_core::test_util::trace::Capture;

    let log = Capture::install();
    test_dead::plane()
        .finished(&test_dead::acquired(), &test_dead::id(9))
        .await
        .expect("a lost acknowledgement is logged, not raised");

    let line = log.only(super::EVENT_TERMINAL_ACK_FAILED);
    let reason = line.field("reason").unwrap_or_default();
    assert!(
        line.field("error_code")
            .is_some_and(|code| !code.is_empty())
    );
    assert!(
        !reason.is_empty() && !reason.contains("[UZ-"),
        "the code is the line's `error_code`, never the reason's prefix: {reason}"
    );
}

/// A fleet whose unreadable config cannot even be recorded is answered no
/// work, and the config's own reason is logged without its code.
#[tokio::test]
async fn an_unreadable_config_refusal_reason_carries_no_code_prefix() {
    use afd_core::test_util::trace::Capture;

    let log = Capture::install();
    let fault = crate::Error::from(
        afd_fleet_runtime::FleetConfig::stored("not a document")
            .expect_err("a document that is not JSON does not parse"),
    );

    test_dead::plane()
        .refuse_unreadable_config(
            &test_dead::acquired(),
            &test_dead::id(9),
            &fault,
            test_dead::AT,
        )
        .await
        .expect("an unrecorded refusal is logged, not raised");

    let line = log.only(super::refuse::EVENT_CONFIG_REFUSAL_UNRECORDED);
    assert_eq!(line.field("config_reason"), Some(fault.told().as_str()));
}
