//! The exchange against a loopback GitHub: the installation read, then the
//! token request, each answered from a script.
//!
//! The server is [`super::fake_github`]'s. These cases mint with an unsigned
//! bearer and grade what the response decides; [`super::signed`] grades the
//! App JWT that signs a real mint.
#![expect(
    clippy::expect_used,
    reason = "the loopback server and client are transport fixture preconditions"
)]

use afd_core::test_util::trace::{Capture, CapturedEvent};
use serde_json::json;
use tracing::Level;

use super::fake_github::{
    CREATED, FakeGithub, GONE, HOLDS_EVERYTHING, HOLDS_NO_CHECKS, IN_AN_HOUR, LATER, OK,
};
use crate::credential::outcome::{Outcome, Retry};

#[tokio::test]
async fn a_narrowed_response_is_delivered_with_the_local_expiry_ceiling() {
    let token = r#"{"token":"ghs_fixture","expires_at":"2026-01-01T00:00:00Z","permissions":{"contents":"read","actions":"read","checks":"read","metadata":"read"},"repositories":[{"full_name":"acme/widgets"}]}"#;
    let github = FakeGithub::answering((OK, HOLDS_EVERYTHING), (CREATED, token)).await;
    let outcome = github.mint().await;
    let minted = outcome.minted().expect("the narrow response mints");

    assert_eq!(minted.token.as_str(), "ghs_fixture");
    assert_eq!(minted.expires_at, IN_AN_HOUR);
    assert!(minted.rotated_refresh_token.is_none());
    assert_eq!(
        github.asked_permissions(),
        vec![json!({"actions": "read", "checks": "read", "contents": "read"})]
    );
}

/// Dimension 1.6 — the App as registered today holds no Checks. The mint asks
/// without it and succeeds, where asking for it failed every mint.
#[tokio::test]
async fn an_installation_without_checks_mints_without_asking_for_them() {
    let token = r#"{"token":"ghs_fixture","permissions":{"contents":"read","actions":"read","metadata":"read"},"repositories":[{"full_name":"acme/widgets"}]}"#;
    let github = FakeGithub::answering((OK, HOLDS_NO_CHECKS), (CREATED, token)).await;

    assert!(github.mint().await.minted().is_some());
    assert_eq!(
        github.asked_permissions(),
        vec![json!({"actions": "read", "contents": "read"})]
    );
}

#[tokio::test]
async fn a_successful_but_overreaching_response_is_discarded() {
    let token = r#"{"token":"ghs_fixture","permissions":{"contents":"write"},"repositories":[{"full_name":"acme/widgets"}]}"#;
    let github = FakeGithub::answering((OK, HOLDS_EVERYTHING), (CREATED, token)).await;
    let capture = Capture::install();

    assert!(matches!(
        github.mint().await,
        Outcome::MintFailed(Retry::Permanent)
    ));
    // One warning an operator can read the reach from, and never the token.
    let events = capture.events();
    let [warning]: [&CapturedEvent; 1] = events
        .iter()
        .filter(|event| event.field("event") == Some("github_mint_overreach"))
        .collect::<Vec<_>>()
        .try_into()
        .expect("exactly one overreach warning");
    assert_eq!(warning.level, Level::WARN);
    for name in ["overreach", "granted", "requested"] {
        assert!(warning.field(name).is_some(), "{name} in {warning:?}");
    }
    assert!(
        warning
            .fields
            .values()
            .all(|value| !value.contains("ghs_fixture")),
        "{warning:?}"
    );
}

/// Dimension 1.7 — a failed installation read is classified as the token
/// request would be, and no token is asked for after it.
#[tokio::test]
async fn an_installation_read_failure_keeps_its_retry_posture() {
    let never_asked = (CREATED, "{}");
    for (case, installation, expected) in [
        ("unauthorised", (401, GONE), Outcome::ReconnectRequired),
        ("uninstalled", (404, GONE), Outcome::ReconnectRequired),
        (
            "unavailable",
            (503, LATER),
            Outcome::MintFailed(Retry::Transient),
        ),
        (
            "unreadable",
            (OK, "not-json"),
            Outcome::MintFailed(Retry::Permanent),
        ),
    ] {
        let github = FakeGithub::answering(installation, never_asked).await;
        let outcome = github.mint().await;
        assert_eq!(
            std::mem::discriminant(&outcome),
            std::mem::discriminant(&expected),
            "{case}: {outcome:?}"
        );
        if let (Outcome::MintFailed(got), Outcome::MintFailed(want)) = (&outcome, &expected) {
            assert_eq!(got, want, "{case}");
        }
        assert!(github.asked_permissions().is_empty(), "{case}");
    }
}

#[tokio::test]
async fn token_request_failures_keep_their_retry_posture() {
    // Any other client error is GitHub refusing the request as made, which
    // asking again unchanged will not fix.
    let refused = FakeGithub::answering((OK, HOLDS_EVERYTHING), (422, GONE)).await;
    assert!(matches!(
        refused.mint().await,
        Outcome::MintFailed(Retry::Permanent)
    ));

    for status in [401, 404] {
        let github = FakeGithub::answering((OK, HOLDS_EVERYTHING), (status, GONE)).await;
        assert!(matches!(github.mint().await, Outcome::ReconnectRequired));
    }

    let unavailable = FakeGithub::answering((OK, HOLDS_EVERYTHING), (503, LATER)).await;
    assert!(matches!(
        unavailable.mint().await,
        Outcome::MintFailed(Retry::Transient)
    ));

    // A success GitHub sent in a shape this client cannot read is refused for
    // good: the same request gets the same body, so a retry only spends
    // another signature to be refused again.
    let malformed = FakeGithub::answering((OK, HOLDS_EVERYTHING), (CREATED, "not-json")).await;
    assert!(matches!(
        malformed.mint().await,
        Outcome::MintFailed(Retry::Permanent)
    ));
}
