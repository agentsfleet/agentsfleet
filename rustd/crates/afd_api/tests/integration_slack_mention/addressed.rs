//! A mention that names a fleet reaches that fleet, whoever else is attached.
//!
//! Split from the sole-subscriber case in the suite root: with two fleets in
//! the channel an unnamed mention is owed a notice, so only the name can pick
//! one, and what is read back is which fleet the ledger admitted it on.

#![cfg(feature = "test-util")]

use super::*;

/// A mention opening with one attached fleet's name is admitted on that fleet
/// with the name removed from the question, though another fleet could answer.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_named_fleet_takes_the_mention_over_its_neighbour() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    fixture
        .fleet(
            &document("responder", CHANNEL, Some("read")),
            FleetStatus::Active.as_str(),
        )
        .await;
    let triage = fixture
        .fleet(
            &document("triage", CHANNEL, Some("read")),
            FleetStatus::Active.as_str(),
        )
        .await;
    let router = fixture.router();

    let body = mention(
        &fixture.team,
        "EvNamed01",
        PERSON,
        &format!("<@{BOT_USER}> Triage: why did it fail?"),
    );
    assert_eq!(deliver(&router, &body).await.status(), StatusCode::OK);

    let admitted = fixture
        .admission(&key(&fixture.team, "EvNamed01"))
        .await
        .expect("the named mention was admitted");
    assert_eq!(admitted.fleet, triage.as_str(), "the name picks the fleet");
    let request: Value =
        serde_json::from_str(&admitted.request_json).expect("the event body is JSON");
    assert_eq!(
        request.get("message").and_then(Value::as_str),
        Some("why did it fail?"),
        "the fleet's name is not part of the question"
    );
    assert_eq!(
        request.pointer("/route/verdict").and_then(Value::as_str),
        Some("addressed")
    );

    fixture.cleanup().await;
}
