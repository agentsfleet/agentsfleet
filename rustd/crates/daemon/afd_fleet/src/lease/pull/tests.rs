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
    let mut issued = 0;
    let answer = test_dead::plane()
        .poll_with(
            crate::lease::OfferRequest {
                runner: &test_dead::id(9),
                held: &[],
                degraded: true,
                now: test_dead::AT,
            },
            None,
            |_lease, _metadata| {
                issued += 1;
            },
        )
        .await
        .expect("a degraded runner is a decision, not a datastore read");
    assert!(matches!(answer, super::Polled::<()>::Empty));
    assert_eq!(issued, 0, "a degraded runner cannot receive an offer");
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
    use af_core::test_util::trace::Capture;

    let log = Capture::install();
    let fault = crate::Error::from(
        af_fleet_runtime::FleetConfig::stored("not a document")
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
