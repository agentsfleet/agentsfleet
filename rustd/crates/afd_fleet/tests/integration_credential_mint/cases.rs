//! Live credential-mint behavior and ordering cases.

use super::*;
use crate::requests;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires a live Postgres; run through `make test-integration-rustd`"]
async fn test_mint_scope_is_the_presenting_runners_lease() {
    // Invariant 2, and the whole reason the wire carries no workspace: a
    // prompt-injected child has nothing to forge, because a lease that is not
    // this runner's resolves to NO ROW rather than to another tenant's
    // workspace.
    support::install_subscriber();
    let fixtures = Fixtures::create_with_queue().await;
    let owner = bound(&fixtures, "{}").await;
    let stranger = bound(&fixtures, "{}").await;
    let leases = fixtures.leases();
    let now = UnixMillis::from_millis(NOW_MS);

    let scope = leases
        .mint_scope(&owner.runner, &owner.lease, now)
        .await
        .expect("the read must succeed")
        .expect("the owner's own lease resolves");
    assert_eq!(scope.workspace_id.as_str(), owner.workspace);
    assert_eq!(scope.fleet_id.as_str(), owner.fleet);
    assert_eq!(&*scope.event_id, EVENT_ID);

    // The IDOR negative: a real, live lease belonging to somebody else.
    assert!(
        leases
            .mint_scope(&stranger.runner, &owner.lease, now)
            .await
            .expect("the read must succeed")
            .is_none(),
        "a foreign lease resolved to a scope"
    );

    // And the lease's own lifetime bounds the authority: past its expiry the
    // same runner presenting the same id resolves to nothing.
    let expired = UnixMillis::from_millis(NOW_MS + LEASE_WINDOW_MS + 1);
    assert!(
        leases
            .mint_scope(&owner.runner, &owner.lease, expired)
            .await
            .expect("the read must succeed")
            .is_none(),
        "an expired lease still authorised a mint"
    );

    fixtures.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires a live Postgres; run through `make test-integration-rustd`"]
async fn test_a_static_handle_mints_from_the_leases_own_workspace() {
    // The positive, and the scope proof: the token's VALUE is what
    // distinguishes the owner's workspace from any other, so a mint that
    // resolved the wrong workspace fails here rather than passing on presence.
    support::install_subscriber();
    let fixtures = Fixtures::create_with_queue().await;
    let owner = bound(&fixtures, "{}").await;
    seed_handle(
        &fixtures,
        &owner.workspace,
        CONNECTOR_STATIC,
        &format!(r#"{{"integration":"static","token":"{OWNER_TOKEN}"}}"#),
    )
    .await;

    let minted = fixtures
        .plane()
        .mint(
            &owner.runner,
            &requests::mint(&owner.lease, CONNECTOR_STATIC),
            UnixMillis::from_millis(NOW_MS),
        )
        .await
        .expect("a connected static handle mints");
    assert_eq!(minted.token.as_str(), OWNER_TOKEN);
    assert!(
        minted.rotated_refresh_token.is_none(),
        "a static handle rotates nothing"
    );

    fixtures.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires a live Postgres; run through `make test-integration-rustd`"]
async fn test_the_grant_gate_refuses_before_the_vault_is_opened() {
    // The ordering property no unit test can reach: an ungranted request must
    // not touch credential bytes. Proven by connecting the integration and
    // withholding only the grant — if the gate ran after the vault read, this
    // would surface as a successful mint.
    support::install_subscriber();
    let fixtures = Fixtures::create_with_queue().await;
    let owner = bound(&fixtures, "{}").await;
    seed_handle(
        &fixtures,
        &owner.workspace,
        CONNECTOR_GITHUB,
        r#"{"integration":"github","installation_id":"42"}"#,
    )
    .await;

    for withheld in ["pending", "revoked"] {
        seed_grant(&fixtures, &owner.fleet, CONNECTOR_GITHUB, withheld).await;
        let refusal = fixtures
            .plane()
            .mint(
                &owner.runner,
                &requests::mint(&owner.lease, CONNECTOR_GITHUB),
                UnixMillis::from_millis(NOW_MS),
            )
            .await
            .expect_err("an ungranted integration must not mint");
        assert_eq!(
            refusal.code(),
            error_code::GRANT_NOT_FOUND,
            "a {withheld} grant was treated as an approval"
        );
    }

    fixtures.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires a live Postgres; run through `make test-integration-rustd`"]
async fn test_an_unconnected_integration_is_not_a_grant_failure() {
    // The two refusals a runner must be able to tell apart: nobody approved it,
    // versus nobody connected it. Approving the grant and storing no handle
    // isolates the second.
    support::install_subscriber();
    let fixtures = Fixtures::create_with_queue().await;
    let owner = bound(&fixtures, "{}").await;
    seed_grant(&fixtures, &owner.fleet, CONNECTOR_GITHUB, "approved").await;

    let refusal = fixtures
        .plane()
        .mint(
            &owner.runner,
            &requests::mint(&owner.lease, CONNECTOR_GITHUB),
            UnixMillis::from_millis(NOW_MS),
        )
        .await
        .expect_err("an unconnected integration must not mint");
    assert_eq!(refusal.code(), error_code::CRED_INTEGRATION_NOT_CONNECTED);

    fixtures.cleanup().await;
}
