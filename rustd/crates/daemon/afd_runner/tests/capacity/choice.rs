//! Replica choices advance only with a committed placement under the host lock.

use afd_crypto::entropy::Entropy;
use afd_runner::capacity::choice::Ticket;
use afd_runner::{Runners, capacity};
use afd_wire::lease::{AcceptedKinds, LeaseKind};
use sqlx::Connection as _;

use super::Fixture;
use super::fixture::LIVE_FOR;

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_kind_preference_survives_a_new_store_handle() {
    let fixture = Fixture::create(2).await;
    let runners = Runners::new(fixture.db.clone(), Entropy::new());
    let ticket = runners
        .claim_offer(&fixture.runner, AcceptedKinds::ALL)
        .await
        .expect("choice read")
        .expect("host exists");
    assert_eq!(
        ticket.order().collect::<Vec<_>>(),
        [LeaseKind::Authoring, LeaseKind::Fleet]
    );

    place_authoring(&fixture, ticket).await;
    let replica = Runners::new(fixture.db.clone(), Entropy::new());
    let next = replica
        .claim_offer(&fixture.runner, AcceptedKinds::ALL)
        .await
        .expect("other replica choice")
        .expect("host exists");
    assert_eq!(
        next.order().collect::<Vec<_>>(),
        [LeaseKind::Fleet, LeaseKind::Authoring]
    );
    let mut connection = fixture.db.acquire().await.expect("next connection");
    let mut tx = connection.begin().await.expect("next transaction");
    let census = capacity::lock(&mut tx, &fixture.runner)
        .await
        .expect("fresh guard")
        .expect("host exists");
    assert!(
        !ticket.permits(census, LeaseKind::Fleet),
        "a stale poll cannot take the other kind"
    );
    assert!(next.permits(census, LeaseKind::Fleet));
    tx.rollback().await.expect("release fresh guard");
    drop(connection);
    fixture.cleanup().await;
}

/// Places an authoring lease under the host lock and advances the preference
/// in the SAME transaction, so the two commit together.
async fn place_authoring(fixture: &Fixture, ticket: Ticket) {
    let mut connection = fixture.db.acquire().await.expect("placement connection");
    let mut tx = connection.begin().await.expect("placement transaction");
    let census = capacity::lock(&mut tx, &fixture.runner)
        .await
        .expect("host guard")
        .expect("host exists");
    assert!(ticket.permits(census, LeaseKind::Authoring));
    fixture
        .authoring(&mut tx, afd_core::clock::after(census.now(), LIVE_FOR))
        .await;
    assert!(
        capacity::choice::record(&mut tx, &fixture.runner, census, LeaseKind::Authoring)
            .await
            .expect("advance alongside placement")
    );
    tx.commit()
        .await
        .expect("commit placement and preference together");
    drop(connection);
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_failed_placement_does_not_advance_preference() {
    let fixture = Fixture::create(1).await;
    let runners = Runners::new(fixture.db.clone(), Entropy::new());
    let ticket = runners
        .claim_offer(&fixture.runner, AcceptedKinds::ALL)
        .await
        .expect("choice")
        .expect("host");
    let mut connection = fixture.db.acquire().await.expect("connection");
    let mut tx = connection.begin().await.expect("transaction");
    let census = capacity::lock(&mut tx, &fixture.runner)
        .await
        .expect("guard")
        .expect("host");
    assert!(
        capacity::choice::record(&mut tx, &fixture.runner, census, LeaseKind::Authoring)
            .await
            .expect("conditional choice update")
    );
    tx.rollback().await.expect("placement failed");
    let mut tx = connection.begin().await.expect("retry transaction");
    let census = capacity::lock(&mut tx, &fixture.runner)
        .await
        .expect("retry guard")
        .expect("host");
    assert!(
        ticket.permits(census, LeaseKind::Authoring),
        "rollback leaves the old offer usable"
    );
    tx.rollback().await.expect("release guard");
    drop(connection);
    let fleet_only = runners
        .claim_offer(&fixture.runner, AcceptedKinds::FLEET)
        .await
        .expect("fleet-only choice")
        .expect("host");
    assert_eq!(fleet_only.order().collect::<Vec<_>>(), [LeaseKind::Fleet]);
    fixture.cleanup().await;
}
