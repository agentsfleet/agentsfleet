//! The refusals the parity walk makes on its way through: a credential that
//! belongs to nobody, an event type this daemon cannot name, and the poll that
//! must not hand the same work out twice.
//!
//! Split from the walk by concern (RULE FLL): that file is the SEQUENCE a
//! runner performs, and these are the three places it asserts the daemon says
//! no. The file cap forced the cut; the seam was already there, because every
//! one of these is reached from the walk and reached nowhere else.

use super::*;

pub(super) async fn assert_unknown_credential_is_refused(http: &reqwest::Client, run: &Scenario) {
    let unknown = http
        .get(format!("{}/v1/runners/me", run.base))
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

pub(super) async fn assert_unsupported_event_ends(http: &reqwest::Client, run: &Scenario) {
    let unsupported = run.enqueue_unsupported_event("future_event_type").await;
    let ended = poll_until(http, run, || async {
        event_column(run, &unsupported, "status").await.as_deref() == Some("gate_blocked")
    })
    .await;
    assert!(
        ended,
        "the unsupported stream entry is ended instead of being retried forever"
    );
}

pub(super) async fn assert_credential_and_duplicate_refusals(
    http: &reqwest::Client,
    run: &Scenario,
    lease_id: &str,
) {
    let mint = post(
        http,
        run,
        "/v1/runners/me/credentials/mint",
        &json!({"lease_id": lease_id, "integration": "anthropic", "scope": null}),
    )
    .await;
    assert_eq!(mint.status().as_u16(), 404);
    assert_eq!(
        field(&json(mint).await, "error_code"),
        &json!("UZ-CRED-001"),
        "a provider credential is not silently treated as a mintable connector"
    );

    let no_second_lease = post(http, run, "/v1/runners/me/leases", &json!({})).await;
    assert_eq!(no_second_lease.status().as_u16(), 200);
    // Scoped to this scenario's fleet rather than asserting `null`. What makes
    // a second poll answer nothing is that the fleet's stream is empty and its
    // readiness mark cleared, NOT a per-runner ceiling: nothing in the daemon
    // stops one runner holding leases on several fleets. So a lease for some
    // other fleet is the shared lane, and only a second lease for THIS fleet
    // would be the duplicate this asserts against.
    assert_no_lease_for_fleet_under_test(run, &json(no_second_lease).await);
}
