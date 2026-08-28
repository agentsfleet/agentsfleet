//! Every broker outcome, mapped to what the runner is told.
//!
//! The Failure Mode this table exists for: each way a mint can fail must reach
//! a TYPED code, never a bare 500 and never another family's copy. Nothing here
//! needs a lease, a vault or a vendor, because the mapping is a pure function of
//! the outcome and which connector produced it.
#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use afd_core::error_code;
use zeroize::Zeroizing;

use super::accept;
use afd_credential::credential::outcome::{Minted, Outcome, Retry};
use afd_credential::secrets::connector::{Connector, Connectors as _, Registry};

/// The connector a handle naming `name` resolves to in the shipped registry.
fn connector(name: &str) -> &'static dyn Connector {
    // The registry is a `#[non_exhaustive]` unit struct, so a caller outside
    // its crate reaches it through `Default` rather than by naming the value.
    // The borrow it hands out lives as long as the declared table it points
    // into.
    const REGISTRY: Registry = Registry::SHIPPED;
    REGISTRY
        .resolve(name)
        .expect("the shipped registry declares it")
}

/// A credential a successful exchange produced.
fn minted() -> Minted {
    Minted {
        token: Zeroizing::new("at_live".to_owned()),
        expires_at_ms: 1_760_003_600_000,
        rotated_refresh_token: None,
    }
}

#[test]
fn a_successful_exchange_carries_its_token_through() {
    let accepted = accept(Outcome::Ok(minted()), Some(connector("github")))
        .expect("a successful outcome is not a refusal");
    assert_eq!(accepted.token.as_str(), "at_live");
}

#[test]
fn githubs_two_refusals_stay_githubs() {
    // An App installation has its own reconnect semantics: a runner meeting
    // UZ-GH-001 knows a HUMAN must reinstall an App, where UZ-GH-002 is
    // something to retry. Collapsing them would lose that.
    let github = Some(connector("github"));
    for (outcome, expected) in [
        (
            Outcome::ReconnectRequired,
            error_code::GH_RECONNECT_REQUIRED,
        ),
        (
            Outcome::MintFailed(Retry::Transient),
            error_code::GH_MINT_FAILED,
        ),
        // Both retry classes answer ONE code — the class is the broker's own
        // concern, and a runner acts the same way on either.
        (
            Outcome::MintFailed(Retry::Permanent),
            error_code::GH_MINT_FAILED,
        ),
    ] {
        let refusal = accept(outcome, github).expect_err("a refusal is not a credential");
        assert_eq!(refusal.code(), expected);
    }
}

#[test]
fn a_refresh_connector_never_tells_a_runner_to_reconnect_github() {
    // The asymmetry this test exists for: zoho, jira and linear share ONE code,
    // and a Zoho failure routing a runner to GitHub's reconnect would send an
    // operator to reinstall an App that was never involved.
    for name in ["zoho", "jira", "linear"] {
        for outcome in [
            Outcome::ReconnectRequired,
            Outcome::MintFailed(Retry::Transient),
            Outcome::MintFailed(Retry::Permanent),
        ] {
            let refusal =
                accept(outcome, Some(connector(name))).expect_err("a refusal is not a credential");
            assert_eq!(
                refusal.code(),
                error_code::CONNECTOR_OAUTH_EXCHANGE_FAILED,
                "{name}"
            );
        }
    }

    // And the two refusals still read differently, because the remedies differ:
    // one is a dead authorization a human reconnects, the other is an exchange
    // worth retrying.
    let reconnect = accept(Outcome::ReconnectRequired, Some(connector("zoho")))
        .expect_err("a refusal is not a credential");
    let failed = accept(
        Outcome::MintFailed(Retry::Transient),
        Some(connector("zoho")),
    )
    .expect_err("a refusal is not a credential");
    assert_ne!(reconnect.detail(), failed.detail());
}

#[test]
fn an_unknown_integration_stays_provider_neutral() {
    // Nothing was exchanged, so there is no provider to be specific about — and
    // the same sentence answers a workspace that connected nothing, which is
    // what keeps the endpoint from being an oracle for either.
    for connector in [None, Some(connector("github")), Some(connector("zoho"))] {
        let refusal = accept(Outcome::UnknownIntegration, connector)
            .expect_err("a refusal is not a credential");
        assert_eq!(refusal.code(), error_code::CRED_INTEGRATION_NOT_CONNECTED);
    }
}

#[test]
fn a_deployment_that_holds_no_platform_credential_says_so() {
    // An OPERATOR's fault, and its own code: the tenant connected an
    // integration this daemon was never given an App or an OAuth client for, so
    // no exchange was attempted and no retry helps.
    for connector in [Some(connector("github")), Some(connector("jira")), None] {
        let refusal =
            accept(Outcome::Unconfigured, connector).expect_err("a refusal is not a credential");
        assert_eq!(refusal.code(), error_code::CRED_BROKER_NOT_CONFIGURED);
    }
}

#[test]
fn no_refusal_answers_an_untyped_failure() {
    // The Failure Mode in one assertion: every outcome that is not a credential
    // reaches a code a runner can act on, and none of them falls through to an
    // internal-error code the client cannot distinguish from a crash.
    let untyped = [
        error_code::INTERNAL_OPERATION_FAILED,
        error_code::INTERNAL_DB_QUERY,
        error_code::INTERNAL_DB_UNAVAILABLE,
    ];
    for outcome in [
        Outcome::UnknownIntegration,
        Outcome::Unconfigured,
        Outcome::ReconnectRequired,
        Outcome::MintFailed(Retry::Transient),
        Outcome::MintFailed(Retry::Permanent),
    ] {
        for named in [None, Some(connector("github")), Some(connector("linear"))] {
            let refusal =
                accept(outcome.clone(), named).expect_err("a refusal is not a credential");
            assert!(
                !untyped.contains(&refusal.code()),
                "{outcome:?} answered {}",
                refusal.code()
            );
            // And every one of them says something, because a detail is not
            // optional on this plane.
            assert!(!refusal.detail().is_empty());
        }
    }
}
