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

/// M252 Dimension 5.1. The saved name is the address: a fleet saved as
/// `ci-dev-repairer` from a definition named `ci-repairer` takes a mention
/// that opens with its saved name, and one opening with its definition's name
/// admits nothing and earns the notice that asks to address it.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn mention_routes_by_the_saved_fleet_name() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let repairer = fixture
        .fleet_saved_as(
            &document("ci-repairer", CHANNEL, Some("write")),
            FleetStatus::Active.as_str(),
            "ci-dev-repairer",
        )
        .await;
    let router = fixture.router();

    let saved = mention(
        &fixture.team,
        "EvSaved01",
        PERSON,
        &format!("<@{BOT_USER}> ci-dev-repairer: open the fix"),
    );
    assert_eq!(deliver(&router, &saved).await.status(), StatusCode::OK);
    let admitted = fixture
        .admission(&key(&fixture.team, "EvSaved01"))
        .await
        .expect("the mention by saved name was admitted");
    assert_eq!(admitted.fleet, repairer.as_str());
    let request: Value =
        serde_json::from_str(&admitted.request_json).expect("the event body is JSON");
    assert_eq!(
        request.get("message").and_then(Value::as_str),
        Some("open the fix")
    );

    let defined = mention(
        &fixture.team,
        "EvDefined01",
        PERSON,
        &format!("<@{BOT_USER}> ci-repairer open the fix"),
    );
    assert_eq!(deliver(&router, &defined).await.status(), StatusCode::OK);
    assert!(
        fixture
            .admission(&key(&fixture.team, "EvDefined01"))
            .await
            .is_none(),
        "the definition's name is not an address"
    );
    let owed = super::notice::owed(&fixture).await;
    assert_eq!(owed.len(), 1, "{owed:?}");
    assert!(
        owed[0].answer.contains("ci-dev-repairer"),
        "the notice names the saved name: {}",
        owed[0].answer
    );

    fixture.cleanup().await;
}

/// M252 Dimension 5.3. A fleet the channel does not subscribe stays
/// unreachable by name, and the channel's write-bound fleet is not addressed.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn mention_cannot_address_a_fleet_outside_the_channel() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    fixture
        .fleet(
            &document("agentbob", "C0OTHERCHANNEL", Some("read")),
            FleetStatus::Active.as_str(),
        )
        .await;
    fixture
        .fleet(
            &document("agentmay", CHANNEL, Some("write")),
            FleetStatus::Active.as_str(),
        )
        .await;
    let router = fixture.router();

    let body = mention(
        &fixture.team,
        "EvOutside01",
        PERSON,
        &format!("<@{BOT_USER}> agentbob: retry"),
    );
    assert_eq!(deliver(&router, &body).await.status(), StatusCode::OK);
    assert!(
        fixture
            .admission(&key(&fixture.team, "EvOutside01"))
            .await
            .is_none(),
        "an unattached fleet is not reached by its name"
    );
    assert_eq!(fixture.admissions().await, 0);

    fixture.cleanup().await;
}
