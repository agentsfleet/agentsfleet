//! A host's run version advances only with a committed placement under the
//! host lock, so a ticket read before another placement cannot place.

use af_crypto::entropy::Entropy;
use afd_db::test_util::mint_id;
use afd_runner::capacity::choice::Ticket;
use afd_runner::{Runners, capacity};
use sqlx::Connection as _;

use super::Fixture;
use super::fixture::LIVE_FOR;

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_placement_on_one_replica_stales_another_replicas_ticket() {
    let fixture = Fixture::create(2).await;
    let runners = Runners::new(fixture.db.clone(), Entropy::new());
    let ticket = runners
        .assignment_ticket(&fixture.runner)
        .await
        .expect("choice read")
        .expect("host exists");

    place(&fixture, ticket).await;
    let replica = Runners::new(fixture.db.clone(), Entropy::new());
    let next = replica
        .assignment_ticket(&fixture.runner)
        .await
        .expect("other replica choice")
        .expect("host exists");
    let mut connection = fixture.db.acquire().await.expect("next connection");
    let mut tx = connection.begin().await.expect("next transaction");
    let fleet = mint_id();
    let census = capacity::lock(
        &mut tx,
        &fixture.runner,
        capacity::Work::Fleet(&fleet),
        None,
    )
    .await
    .expect("fresh guard")
    .expect("host exists");
    assert!(
        !ticket.permits(census),
        "a stale poll cannot place after another placement"
    );
    assert!(next.permits(census));
    tx.rollback().await.expect("release fresh guard");
    drop(connection);
    fixture.cleanup().await;
}

/// Places a run under the host lock and advances the run version in the SAME
/// transaction, so the two commit together.
async fn place(fixture: &Fixture, ticket: Ticket) {
    let mut connection = fixture.db.acquire().await.expect("placement connection");
    let mut tx = connection.begin().await.expect("placement transaction");
    let fleet = mint_id();
    let census = capacity::lock(
        &mut tx,
        &fixture.runner,
        capacity::Work::Fleet(&fleet),
        None,
    )
    .await
    .expect("host guard")
    .expect("host exists");
    assert!(ticket.permits(census));
    fixture
        .place(&mut tx, af_core::clock::after(census.now(), LIVE_FOR))
        .await;
    assert!(
        capacity::choice::record(&mut tx, &fixture.runner, census)
            .await
            .expect("advance alongside placement")
    );
    tx.commit()
        .await
        .expect("commit placement and run version together");
    drop(connection);
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_failed_placement_leaves_the_ticket_usable() {
    let fixture = Fixture::create(1).await;
    let runners = Runners::new(fixture.db.clone(), Entropy::new());
    let ticket = runners
        .assignment_ticket(&fixture.runner)
        .await
        .expect("choice")
        .expect("host");
    let fleet = mint_id();
    let mut connection = fixture.db.acquire().await.expect("connection");
    let mut tx = connection.begin().await.expect("transaction");
    let census = capacity::lock(
        &mut tx,
        &fixture.runner,
        capacity::Work::Fleet(&fleet),
        None,
    )
    .await
    .expect("guard")
    .expect("host");
    assert!(
        capacity::choice::record(&mut tx, &fixture.runner, census)
            .await
            .expect("conditional choice update")
    );
    tx.rollback().await.expect("placement failed");
    let mut tx = connection.begin().await.expect("retry transaction");
    let census = capacity::lock(
        &mut tx,
        &fixture.runner,
        capacity::Work::Fleet(&fleet),
        None,
    )
    .await
    .expect("retry guard")
    .expect("host");
    assert!(
        ticket.permits(census),
        "rollback leaves the old assignment usable"
    );
    tx.rollback().await.expect("release guard");
    drop(connection);
    fixture.cleanup().await;
}
