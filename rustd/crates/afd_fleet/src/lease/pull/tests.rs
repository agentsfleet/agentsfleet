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
