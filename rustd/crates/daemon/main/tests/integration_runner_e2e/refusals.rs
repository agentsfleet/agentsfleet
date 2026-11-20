//! The refusals the parity walk makes on its way through: a credential that
//! belongs to nobody, an event type this daemon cannot name, and the poll that
//! must not hand the same work out twice.
//!
//! Split from the walk by concern (RULE FLL): that file is the SEQUENCE a
//! runner performs, and these are the three places it asserts the daemon says
//! no. The file cap forced the cut; the seam was already there, because every
//! one of these is reached from the walk and reached nowhere else.

use super::*;

pub(super) async fn assert_unknown_credential_is_refused(http: &reqwest::Client, scene: &Scenario) {
    let unknown = http
        .get(format!("{}/v1/runners/me", scene.base))
        .bearer_auth(UNKNOWN_TOKEN)
        .send()
        .await
        .expect("the daemon answers an unknown credential");
    assert_eq!(
        unknown.status().as_u16(),
        401,
        "a well-formed token belonging to no row is refused by the directory"
    );
}

pub(super) async fn assert_unsupported_event_ends(scene: &Scenario) {
    let unsupported = scene.enqueue_unsupported_event("future_event_type").await;
    let ended = wait_without_assignment(scene, async || {
        event_column(scene, &unsupported, "status").await.as_deref() == Some("gate_blocked")
    })
    .await;
    assert!(
        ended,
        "the unsupported stream entry is ended instead of being retried forever"
    );
}

pub(super) async fn assert_credential_and_duplicate_refusals(
    http: &reqwest::Client,
    scene: &Scenario,
    run_id: &str,
) {
    let mint = post(
        http,
        scene,
        "/v1/runners/me/credentials/mint",
        &json!({"run_id": run_id, "integration": "anthropic", "scope": null}),
    )
    .await;
    assert_eq!(mint.status().as_u16(), 404);
    assert_eq!(
        field(&json(mint).await, "error_code"),
        &json!("UZ-CRED-001"),
        "a provider credential is not silently treated as a mintable connector"
    );

    scene.native_session().await.capacity(2);
    assert!(
        !wait_without_assignment(scene, async || { false }).await,
        "a second free worker must not receive the same fleet's active admission"
    );
    assert_eq!(run_rows(scene).await, 1);
}
