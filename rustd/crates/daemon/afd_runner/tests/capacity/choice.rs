//! Replica choices advance only with a committed placement under the host lock.

use af_crypto::entropy::Entropy;
use af_wire::run::{AcceptedKinds, RunKind};
use afd_runner::capacity::choice::Ticket;
use afd_runner::{Runners, capacity};
use sqlx::Connection as _;

use super::Fixture;
use super::fixture::LIVE_FOR;

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_kind_preference_survives_a_new_store_handle() {
    let fixture = Fixture::create(2).await;
    let runners = Runners::new(fixture.db.clone(), Entropy::new());
    let ticket = runners
        .assignment_ticket(&fixture.runner, AcceptedKinds::ALL)
        .await
        .expect("choice read")
        .expect("host exists");
    assert_eq!(
        ticket.order().collect::<Vec<_>>(),
        [RunKind::Setup, RunKind::Fleet]
    );

    place_setup(&fixture, ticket).await;
    let replica = Runners::new(fixture.db.clone(), Entropy::new());
    let next = replica
        .assignment_ticket(&fixture.runner, AcceptedKinds::ALL)
        .await
        .expect("other replica choice")
        .expect("host exists");
    assert_eq!(
        next.order().collect::<Vec<_>>(),
        [RunKind::Fleet, RunKind::Setup]
    );
    let mut connection = fixture.db.acquire().await.expect("next connection");
    let mut tx = connection.begin().await.expect("next transaction");
    let census = capacity::lock(&mut tx, &fixture.runner, capacity::Work::Setup, None)
        .await
        .expect("fresh guard")
        .expect("host exists");
    assert!(
        !ticket.permits(census, RunKind::Fleet),
        "a stale poll cannot take the other kind"
    );
    assert!(next.permits(census, RunKind::Fleet));
    tx.rollback().await.expect("release fresh guard");
    drop(connection);
    fixture.cleanup().await;
}

/// Places a setup run under the host lock and advances the preference
/// in the SAME transaction, so the two commit together.
async fn place_setup(fixture: &Fixture, ticket: Ticket) {
    let mut connection = fixture.db.acquire().await.expect("placement connection");
    let mut tx = connection.begin().await.expect("placement transaction");
    let census = capacity::lock(&mut tx, &fixture.runner, capacity::Work::Setup, None)
        .await
        .expect("host guard")
        .expect("host exists");
    assert!(ticket.permits(census, RunKind::Setup));
    fixture
        .setup(&mut tx, af_core::clock::after(census.now(), LIVE_FOR))
        .await;
    assert!(
        capacity::choice::record(&mut tx, &fixture.runner, census, RunKind::Setup)
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
        .assignment_ticket(&fixture.runner, AcceptedKinds::ALL)
        .await
        .expect("choice")
        .expect("host");
    let mut connection = fixture.db.acquire().await.expect("connection");
    let mut tx = connection.begin().await.expect("transaction");
    let census = capacity::lock(&mut tx, &fixture.runner, capacity::Work::Setup, None)
        .await
        .expect("guard")
        .expect("host");
    assert!(
        capacity::choice::record(&mut tx, &fixture.runner, census, RunKind::Setup)
            .await
            .expect("conditional choice update")
    );
    tx.rollback().await.expect("placement failed");
    let mut tx = connection.begin().await.expect("retry transaction");
    let census = capacity::lock(&mut tx, &fixture.runner, capacity::Work::Setup, None)
        .await
        .expect("retry guard")
        .expect("host");
    assert!(
        ticket.permits(census, RunKind::Setup),
        "rollback leaves the old assignment usable"
    );
    tx.rollback().await.expect("release guard");
    drop(connection);
    let fleet_only = runners
        .assignment_ticket(&fixture.runner, AcceptedKinds::FLEET)
        .await
        .expect("fleet-only choice")
        .expect("host");
    assert_eq!(fleet_only.order().collect::<Vec<_>>(), [RunKind::Fleet]);
    fixture.cleanup().await;
}
