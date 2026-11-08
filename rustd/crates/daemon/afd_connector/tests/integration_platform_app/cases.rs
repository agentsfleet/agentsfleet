//! What the reader answers, over the rows the fixture sealed.

use super::*;

use std::collections::BTreeSet;

#[tokio::test]
#[ignore = "needs a live Postgres: make test-integration-rustd"]
async fn a_configured_app_answers_the_bag_it_was_sealed_with() {
    // Two providers with different values through one reader. A single bag
    // cannot tell a vault lookup from a hardcoded answer: both assertions
    // below pass against a constant if only one of them exists.
    let deployment = Deployment::create().await;
    deployment.configure(Provider::Slack, SLACK_BAG).await;
    deployment.configure(Provider::Jira, JIRA_BAG).await;
    let apps = deployment.apps();

    let slack = apps
        .credentials(&deployment.admin, Provider::Slack)
        .await
        .expect("a configured app reads")
        .expect("Slack is configured");
    assert_eq!(slack.client_id, "slack-client-1");
    assert_eq!(slack.client_secret.expose(), b"slack-secret-1");

    let jira = apps
        .credentials(&deployment.admin, Provider::Jira)
        .await
        .expect("a configured app reads")
        .expect("Jira is configured");
    assert_eq!(jira.client_id, "jira-client-2");
    assert_eq!(jira.client_secret.expose(), b"jira-secret-2");

    deployment.cleanup().await;
}

#[tokio::test]
#[ignore = "needs a live Postgres: make test-integration-rustd"]
async fn the_signing_secret_is_read_from_the_same_bag_and_only_when_present() {
    // The pair is the claim. Slack registers all three at once, Jira registers
    // two — so an implementation that answered the OAuth secret, or answered
    // any secret it found, passes the first assertion and fails the second.
    let deployment = Deployment::create().await;
    deployment.configure(Provider::Slack, SLACK_BAG).await;
    deployment.configure(Provider::Jira, JIRA_BAG).await;
    let apps = deployment.apps();

    let signing = apps
        .signing_secret(&deployment.admin, Provider::Slack)
        .await
        .expect("a configured app reads")
        .expect("Slack registers a signing secret");
    assert_eq!(signing.expose(), b"slack-signing-1");

    assert!(
        apps.signing_secret(&deployment.admin, Provider::Jira)
            .await
            .expect("a configured app reads")
            .is_none(),
        "a bag with no signing secret must not answer one from another field"
    );

    deployment.cleanup().await;
}

#[tokio::test]
#[ignore = "needs a live Postgres: make test-integration-rustd"]
async fn a_provider_with_no_bag_is_unconfigured_for_both_readers() {
    // The common case, and the one that must never raise: a deployment that
    // connected nothing is not an incident. Both readers go through the same
    // `bag`, so both are asserted — a refusal added to either would surface
    // here rather than as a 500 on a settings page.
    let deployment = Deployment::create().await;
    let apps = deployment.apps();

    assert!(
        apps.credentials(&deployment.admin, Provider::GitHub)
            .await
            .expect("an absent bag is not a fault")
            .is_none(),
        "an unconfigured provider answered credentials"
    );
    assert!(
        apps.signing_secret(&deployment.admin, Provider::GitHub)
            .await
            .expect("an absent bag is not a fault")
            .is_none(),
        "an unconfigured provider answered a signing secret"
    );

    deployment.cleanup().await;
}

#[tokio::test]
#[ignore = "needs a live Postgres: make test-integration-rustd"]
async fn a_bag_without_its_oauth_pair_is_not_a_configured_app() {
    // The dangerous direction. A half-saved bag answering `Some` would start a
    // connect with an empty client secret and fail at the vendor, where the
    // operator reads a 401 instead of "you have not finished configuring this".
    // The signing secret IS present, so this also proves the two fields are
    // judged independently rather than by the bag's mere existence.
    let deployment = Deployment::create().await;
    deployment
        .configure(Provider::Linear, SIGNING_ONLY_BAG)
        .await;
    let apps = deployment.apps();

    assert!(
        apps.credentials(&deployment.admin, Provider::Linear)
            .await
            .expect("a half-saved bag is not a fault")
            .is_none(),
        "a bag with no client id started a connect"
    );
    assert_eq!(
        apps.signing_secret(&deployment.admin, Provider::Linear)
            .await
            .expect("a half-saved bag is not a fault")
            .map(|secret| secret.expose().to_vec()),
        Some(b"linear-signing-3".to_vec()),
        "the field that IS saved must still read"
    );

    deployment.cleanup().await;
}

#[tokio::test]
#[ignore = "needs a live Postgres: make test-integration-rustd"]
async fn provisioned_names_the_providers_holding_a_bag_and_no_others() {
    // Set equality, not containment: an implementation that answered
    // `Provider::ALL` passes any "contains Slack" assertion while telling an
    // operator every connector is ready. The unconfigured three are named by
    // their absence from an exact set.
    let deployment = Deployment::create().await;
    deployment.configure(Provider::Slack, SLACK_BAG).await;
    deployment.configure(Provider::Jira, JIRA_BAG).await;
    let apps = deployment.apps();

    let provisioned = apps
        .provisioned(Some(&deployment.admin))
        .await
        .expect("the directory listing reads");
    assert_eq!(
        provisioned,
        BTreeSet::from([Provider::Slack, Provider::Jira]),
        "the listing must name exactly the providers holding a bag"
    );

    // A deployment with no admin workspace holds no apps — every provider
    // reads as not configured, which is exactly true.
    assert!(
        apps.provisioned(None)
            .await
            .expect("an absent admin workspace is not a fault")
            .is_empty(),
        "a deployment with no admin workspace named a configured provider"
    );

    deployment.cleanup().await;
}
