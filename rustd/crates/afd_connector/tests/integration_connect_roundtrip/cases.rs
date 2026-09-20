//! The verbs, driven over the deployment the parent stood up.

use super::*;

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_jira_connect_lands_a_grant_naming_the_site_it_was_scoped_to() {
    let vendor = FakeAtlassian::serving().await;
    let round = Round::create(&vendor).await;
    round.configure_jira().await;
    let now = UnixMillis::from_millis(NOW_MS);
    let secret = signing_secret();

    let consent = consent_of(
        round
            .connectors
            .start(
                Starting {
                    admin: &round.admin,
                    workspace: &round.workspace,
                    provider: Provider::Jira,
                    subject: SUBJECT,
                    redirect_uri: REDIRECT_URI,
                    secret: &secret,
                },
                now,
            )
            .await
            .expect("a configured provider starts"),
    );

    let verified = round
        .connectors
        .verify(Provider::Jira, &secret, &state_of(&consent), SUBJECT, now)
        .expect("this deployment's own state verifies for its own starter");
    let spent = round
        .connectors
        .spend(Provider::Jira, &verified)
        .await
        .expect("the nonce store answers")
        .expect("a freshly minted nonce has not been spent");

    let landed = round
        .connectors
        .finish(
            Finishing {
                admin: &round.admin,
                provider: Provider::Jira,
                spent: &spent,
                code: CODE,
                location: None,
                installation_id: None,
                redirect_uri: REDIRECT_URI,
            },
            now,
        )
        .await
        .expect("the loopback vendor answers a redeemable code");
    assert_eq!(landed, Landed::Connected);

    // The handoff, asserted where it lands rather than where it is passed.
    let grant = round.landed_grant().await;
    assert_eq!(field_of(&grant, "integration"), Some("jira"));
    assert_eq!(field_of(&grant, "access_token"), Some(vendor::ACCESS_TOKEN));
    assert_eq!(
        field_of(&grant, "refresh_token"),
        Some(vendor::REFRESH_TOKEN)
    );
    // The second call's whole purpose: a grant that skipped it names no site.
    assert_eq!(field_of(&grant, "cloud_id"), Some(vendor::CLOUD_ID));
    assert_eq!(field_of(&grant, "site_url"), Some(vendor::SITE_URL));
    assert_eq!(field_of(&grant, "label"), Some(vendor::SITE_NAME));

    assert_eq!(
        vendor.redeemed_code().await.as_deref(),
        Some(CODE),
        "the vendor was handed a code this connect did not mint"
    );

    round.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_spent_nonce_cannot_be_spent_again() {
    // The single-use half of the round trip. A state is a bearer value the
    // browser carried, so a replayed callback must not land a second grant —
    // and the nonce is the only thing that says the first one happened.
    let vendor = FakeAtlassian::serving().await;
    let round = Round::create(&vendor).await;
    round.configure_jira().await;
    let now = UnixMillis::from_millis(NOW_MS);
    let secret = signing_secret();

    let consent = consent_of(
        round
            .connectors
            .start(
                Starting {
                    admin: &round.admin,
                    workspace: &round.workspace,
                    provider: Provider::Jira,
                    subject: SUBJECT,
                    redirect_uri: REDIRECT_URI,
                    secret: &secret,
                },
                now,
            )
            .await
            .expect("a configured provider starts"),
    );
    let state = state_of(&consent);

    let first = round
        .connectors
        .verify(Provider::Jira, &secret, &state, SUBJECT, now)
        .expect("the state verifies");
    assert!(
        round
            .connectors
            .spend(Provider::Jira, &first)
            .await
            .expect("the nonce store answers")
            .is_some(),
        "the first spend must succeed"
    );

    // The SAME state, presented again: it still verifies — the signature did
    // not change — and that is exactly why the nonce is a separate question.
    let replayed = round
        .connectors
        .verify(Provider::Jira, &secret, &state, SUBJECT, now)
        .expect("a replayed state still carries a valid signature");
    assert!(
        round
            .connectors
            .spend(Provider::Jira, &replayed)
            .await
            .expect("the nonce store answers")
            .is_none(),
        "a spent nonce was spendable twice; a replayed callback lands a second grant"
    );

    round.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_unconfigured_provider_starts_nothing() {
    // The value-not-error arm: an operator who has not set a connector up is
    // not a failure, and the caller renders `UZ-CONN-001` for it. Nothing is
    // seeded here, which is the whole fixture.
    let vendor = FakeAtlassian::serving().await;
    let round = Round::create(&vendor).await;
    let secret = signing_secret();

    let started = round
        .connectors
        .start(
            Starting {
                admin: &round.admin,
                workspace: &round.workspace,
                provider: Provider::Jira,
                subject: SUBJECT,
                redirect_uri: REDIRECT_URI,
                secret: &secret,
            },
            UnixMillis::from_millis(NOW_MS),
        )
        .await
        .expect("an unconfigured provider is not a fault");
    assert!(
        matches!(started, Started::NotConfigured),
        "an unconfigured provider answered a consent screen"
    );

    round.cleanup().await;
}

/// The consent screen a started connect answers with.
///
/// An `Option` rather than a `let ... else { panic! }`: the two read the same
/// at the call site, and this one does not trade the indexing lint for the
/// panic one.
fn consent_of(started: Started) -> String {
    match started {
        Started::Consent(url) => Some(url),
        Started::NotConfigured => None,
    }
    .expect("a configured provider answers a consent screen")
}

/// One string field of a sealed handle.
///
/// `get` rather than indexing, which also tells an ABSENT field from one
/// carrying the wrong value — the distinction every assertion above makes.
fn field_of<'g>(grant: &'g serde_json::Value, name: &str) -> Option<&'g str> {
    grant.get(name).and_then(serde_json::Value::as_str)
}
