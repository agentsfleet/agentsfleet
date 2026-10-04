#![expect(
    clippy::unwrap_used,
    reason = "test target: a fixture that cannot be built is a broken test"
)]

use afd_core::id::Uuid7;
use afr_egress::Mint as _;

use super::{LeaseMint, mint};
use crate::client::Verb;
use crate::error;
use crate::test_support::{Answer, LEASE_ID, drain, json, plane};

#[tokio::test]
async fn a_mint_names_the_lease_and_never_prints_its_token() {
    let (plane, mut calls) =
        plane(|_call| json(&serde_json::json!({"token": "ghs_secret", "expires_at_ms": 99})));

    let minted = mint(
        &plane,
        &Uuid7::parse(LEASE_ID).unwrap(),
        "github",
        Some("contents:read"),
    )
    .await
    .unwrap();

    assert_eq!(minted.expose(), "ghs_secret");
    assert_eq!(minted.expires_at().as_millis(), 99);
    assert!(!format!("{minted:?}").contains("ghs_secret"));
    let sent: serde_json::Value =
        serde_json::from_slice(&drain(&mut calls).remove(0).body.unwrap()).unwrap();
    assert_eq!(
        sent,
        serde_json::json!({"lease_id": LEASE_ID, "integration": "github", "scope": "contents:read"})
    );
}

#[tokio::test]
async fn a_refused_mint_is_not_retried() {
    let (plane, mut calls) = plane(|_call| Answer::Fail(error::refused(Verb::Mint, 403, None)));

    let refused = mint(&plane, &Uuid7::parse(LEASE_ID).unwrap(), "github", None)
        .await
        .unwrap_err();

    assert!(!refused.is_retryable());
    assert_eq!(drain(&mut calls).len(), 1);
}

#[tokio::test]
async fn a_lease_mint_hands_the_model_the_daemons_code_first() {
    let lease_id = Uuid7::parse(LEASE_ID).unwrap();
    let drift = afd_core::error_code::REPAIR_BINDING_DRIFT;
    let (plane, _calls) =
        plane(move |_call| Answer::Fail(error::refused(Verb::Mint, 403, Some(drift))));

    let refused = LeaseMint::new(&plane, &lease_id)
        .mint("github")
        .await
        .unwrap_err();

    assert_eq!(
        refused.detail(),
        "the credential could not be minted: UZ-REPAIR-011: the daemon refused the mint (403)"
    );
    assert_eq!(refused.code(), drift, "the daemon's code, kept");
}

#[tokio::test]
async fn a_lease_mint_answers_the_minted_token() {
    let lease_id = Uuid7::parse(LEASE_ID).unwrap();
    let (plane, _calls) =
        plane(|_call| json(&serde_json::json!({"token": "ghs_secret", "expires_at_ms": 99})));

    let minted = LeaseMint::new(&plane, &lease_id)
        .mint("github")
        .await
        .unwrap();

    assert_eq!(
        (minted.expose(), minted.expires_at().as_millis()),
        ("ghs_secret", 99)
    );
}

/// Builds the failure the fake daemon answers a mint with.
type Failure = fn() -> crate::Error;

#[tokio::test]
async fn a_lease_mint_names_the_status_or_the_unreached_daemon() {
    let lease_id = Uuid7::parse(LEASE_ID).unwrap();
    let cases: [(Failure, &str); 2] = [
        (
            || error::refused(Verb::Mint, 403, None),
            "the credential could not be minted: the daemon refused the mint (403)",
        ),
        (
            || error::unavailable(Verb::Mint, 503),
            "the credential could not be minted: the daemon could not be reached to mint the \
             credential",
        ),
    ];

    for (failure, detail) in cases {
        let (plane, _calls) = plane(move |_call| Answer::Fail(failure()));
        let refused = LeaseMint::new(&plane, &lease_id)
            .mint("github")
            .await
            .unwrap_err();
        assert_eq!(refused.detail(), detail);
    }
}
