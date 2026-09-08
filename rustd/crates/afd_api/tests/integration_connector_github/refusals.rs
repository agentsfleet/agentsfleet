//! What GitHub connect REFUSES, and what it leaves behind when it does.
//!
//! Split from the binding cases beside it: those assert a handle and a routing
//! row exist afterwards, and every case here asserts the opposite — the
//! ownership code on the wire, no handle in the vault, the routing table as it
//! was. [`assert_ownership_refused`] is the shared shape of that claim, which
//! is why it lives here rather than in the parent.

#![cfg(feature = "test-util")]

use super::*;

/// The refusal every ownership case answers, and the state it leaves.
async fn assert_ownership_refused(
    fixture: &Fixture,
    response: axum::response::Response,
    account: &str,
) {
    let status = response.status();
    let document = json_body(response).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{document}");
    assert_eq!(
        document.get("error_code").and_then(Value::as_str),
        Some(error_code::CONNECTOR_INSTALLATION_OWNERSHIP.as_str()),
        "{document}"
    );
    assert!(
        fixture.grant(PROVIDER).await.is_none(),
        "a refused ownership seals no handle"
    );
    assert!(
        !fixture
            .routed_to(PROVIDER, account)
            .await
            .contains(&fixture.workspace.as_str().to_owned()),
        "a refused ownership routes nothing to the workspace"
    );
}

#[tokio::test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn a_claim_the_token_does_not_open_is_refused_as_ownership() {
    let fixture = github_fixture().await;
    let installation = fresh_installation();
    let provider = FakeProvider::answering_with_reads(
        &[EXCHANGE],
        vec![probe(&installation, 404, PROBE_REFUSED)],
    )
    .await;
    let router = fixture.router(&provider);

    let state = start_connect(&router, &fixture, PROVIDER).await;
    let refused = complete_with(
        &router,
        &fixture,
        PROVIDER,
        &state,
        &format!("&installation_id={installation}"),
    )
    .await;
    assert_ownership_refused(&fixture, refused, &installation).await;

    provider.close();
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn a_listing_of_none_binds_nothing() {
    // The App is installed nowhere this person reaches, so there is nothing to
    // restore. The Zig daemon sent the browser on to GitHub's install page
    // here; the Rust tree carries no App slug, so the person is refused and
    // told to install the App first. `docs/AUTH.md` carries the divergence.
    let fixture = github_fixture().await;
    let installation = fresh_installation();
    let provider = FakeProvider::answering_with_reads(&[EXCHANGE], vec![listing_none()]).await;
    let router = fixture.router(&provider);

    let state = start_connect(&router, &fixture, PROVIDER).await;
    let refused = complete(&router, &fixture, PROVIDER, &state).await;
    assert_ownership_refused(&fixture, refused, &installation).await;

    provider.close();
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn a_listing_of_several_binds_neither_of_them() {
    // Choosing one would be choosing an organisation for the person, which is
    // how one team's pull requests reach another team's workspace. Both listed
    // installations are asserted unbound, so the refusal cannot be passing
    // because it happened to skip the first.
    let fixture = github_fixture().await;
    let first = fresh_installation();
    let second = fresh_installation();
    let provider =
        FakeProvider::answering_with_reads(&[EXCHANGE], vec![listing_two(&first, &second)]).await;
    let router = fixture.router(&provider);

    let state = start_connect(&router, &fixture, PROVIDER).await;
    let refused = complete(&router, &fixture, PROVIDER, &state).await;
    assert_ownership_refused(&fixture, refused, &first).await;
    assert_eq!(
        fixture.routed_to(PROVIDER, &second).await,
        Vec::<String>::new(),
        "the second listed installation is not bound either"
    );

    provider.close();
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn an_installation_another_workspace_routes_is_refused_and_stays_where_it_is() {
    // The exclusive claim. Slack's routing row follows the latest connect;
    // GitHub's does not, and the difference is asserted from both sides: the
    // connecting workspace gets the ownership code and no handle, and the
    // holding workspace's row is exactly as it was.
    let fixture = github_fixture().await;
    let installation = fresh_installation();
    fixture.route_elsewhere(PROVIDER, &installation).await;
    let provider =
        FakeProvider::answering_with_reads(&[EXCHANGE], vec![listing_one(&installation)]).await;
    let router = fixture.router(&provider);

    let state = start_connect(&router, &fixture, PROVIDER).await;
    let refused = complete(&router, &fixture, PROVIDER, &state).await;
    assert_ownership_refused(&fixture, refused, &installation).await;
    assert_eq!(
        provider.exchanges(),
        1,
        "the refusal is the routing row's, past the vendor"
    );
    assert_eq!(
        fixture.routed_to(PROVIDER, &installation).await,
        vec![fixture.admin_workspace().to_owned()],
        "the holding workspace keeps the installation"
    );

    provider.close();
    fixture.cleanup().await;
}

/// The refusal a vendor that will not ANSWER produces, and the state it leaves.
///
/// Deliberately not [`assert_ownership_refused`]: the two are different
/// sentences to a person and different codes on the wire (RULE ECL). Ownership
/// is the vendor answering and the answer being none, several, or an
/// installation this token cannot open — the person installs the App or signs
/// in as the owner. This is the vendor declining the question, which no action
/// of theirs changes.
async fn assert_listing_declined(fixture: &Fixture, response: axum::response::Response) {
    let status = response.status();
    let document = json_body(response).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY, "{document}");
    assert_eq!(
        document.get("error_code").and_then(Value::as_str),
        Some(error_code::CONNECTOR_INSTALLATION_LISTING_FAILED.as_str()),
        "a vendor that declined the listing is not an ownership refusal: {document}"
    );
    assert!(
        fixture.grant(PROVIDER).await.is_none(),
        "a declined listing seals no handle"
    );
}

#[tokio::test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn a_listing_the_vendor_declines_is_not_an_ownership_refusal() {
    // The shape the live defect had: the exchange SUCCEEDED, the token is in
    // hand, and `api.github.com` refused the next call — 403 "Request
    // forbidden by administrative rules", because the request carried no
    // `User-Agent`. Reported under the exchange's code, it sent a live
    // diagnosis to a client secret that was working.
    let fixture = github_fixture().await;
    let provider =
        FakeProvider::answering_with_reads(&[EXCHANGE], vec![listing_declined(403)]).await;
    let router = fixture.router(&provider);

    let state = start_connect(&router, &fixture, PROVIDER).await;
    let refused = complete(&router, &fixture, PROVIDER, &state).await;
    assert_listing_declined(&fixture, refused).await;

    provider.close();
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn a_claimed_installation_the_vendor_cannot_answer_for_is_not_ownership() {
    // 401, 403 and 404 on this probe mean "this token does not open that
    // installation" — an ownership answer. Every OTHER status is the vendor
    // having a bad minute, and collapsing the two is what let a malformed
    // request read as a person lacking access.
    let fixture = github_fixture().await;
    let installation = fresh_installation();
    let provider = FakeProvider::answering_with_reads(
        &[EXCHANGE],
        vec![probe(&installation, 500, VENDOR_DECLINED)],
    )
    .await;
    let router = fixture.router(&provider);

    let state = start_connect(&router, &fixture, PROVIDER).await;
    let refused = complete_with(
        &router,
        &fixture,
        PROVIDER,
        &state,
        &format!("&installation_id={installation}"),
    )
    .await;
    assert_listing_declined(&fixture, refused).await;

    provider.close();
    fixture.cleanup().await;
}
