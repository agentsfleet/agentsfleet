//! What a SECOND connect does to the first one's rows.
//!
//! The exclusive claim is exclusive to other WORKSPACES, never to the one
//! holding it, so both cases here land rather than refuse. What they differ on
//! is what survives: reconnecting the same installation must not double its
//! row, and connecting a different one must not leave the old row behind —
//! the vault holds a single handle per provider, so a routing row it can no
//! longer spend a credential for is a delivery routed into a failure.

#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use super::*;

#[tokio::test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn a_reconnect_of_the_same_installation_refreshes_rather_than_refuses() {
    // Exclusive to OTHER workspaces, not to the one that holds it: a person
    // reconnecting after a rotation must land, and the row must not double.
    let fixture = github_fixture().await;
    let installation = fresh_installation();
    let provider =
        FakeProvider::answering_with_reads(&[EXCHANGE, EXCHANGE], vec![listing_one(&installation)])
            .await;
    let router = fixture.router(&provider);

    for _connect in 0..2 {
        let state = start_connect(&router, &fixture, PROVIDER).await;
        let landed = complete(&router, &fixture, PROVIDER, &state).await;
        let status = landed.status();
        assert_eq!(status, StatusCode::FOUND, "{}", json_body(landed).await);
    }
    assert_eq!(provider.exchanges(), 2);
    assert_eq!(
        fixture.routed_to(PROVIDER, &installation).await,
        vec![fixture.workspace.as_str().to_owned()],
        "one row, still this workspace's"
    );
    assert_eq!(
        fixture.secret_names().await,
        vec![PROVIDER.grant_key().to_owned()],
        "one handle, under the provider's key"
    );

    provider.close();
    fixture.cleanup().await;
}

/// A workspace connecting a SECOND installation lets go of the first.
///
/// The vault holds ONE handle per workspace and provider, so a surviving row
/// for the old installation would resolve its deliveries to this workspace and
/// answer them with a credential minted for the new one. The runs fail at the
/// vendor and nothing in the product reads as broken, which is what makes it
/// worth a test rather than a comment.
///
/// Two providers rather than one listing served twice: the fake registers one
/// route per path and repeats its body, so a single provider cannot answer
/// `/user/installations` differently on the second connect. Standing a second
/// one up is what a person changing which installation they authorize looks
/// like from the daemon's side anyway.
#[tokio::test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn a_second_installation_replaces_the_first_rather_than_joining_it() {
    let fixture = github_fixture().await;
    let first = fresh_installation();
    let second = fresh_installation();

    for installation in [&first, &second] {
        let provider =
            FakeProvider::answering_with_reads(&[EXCHANGE], vec![listing_one(installation)]).await;
        let router = fixture.router(&provider);
        let state = start_connect(&router, &fixture, PROVIDER).await;
        let landed = complete(&router, &fixture, PROVIDER, &state).await;
        let status = landed.status();
        assert_eq!(status, StatusCode::FOUND, "{}", json_body(landed).await);
        provider.close();
    }

    assert!(
        fixture.routed_to(PROVIDER, &first).await.is_empty(),
        "the first installation is no longer routed to a workspace that cannot \
         mint a token for it"
    );
    assert_eq!(
        fixture.routed_to(PROVIDER, &second).await,
        vec![fixture.workspace.as_str().to_owned()],
        "the second installation is the one this workspace routes"
    );
    assert_eq!(
        fixture.secret_names().await,
        vec![PROVIDER.grant_key().to_owned()],
        "one handle, and the routing table agrees with what it can spend"
    );

    fixture.cleanup().await;
}
