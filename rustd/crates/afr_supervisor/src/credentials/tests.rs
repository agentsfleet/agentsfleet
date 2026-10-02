#![expect(
    clippy::unwrap_used,
    reason = "test target: a fixture that cannot be built is a broken test"
)]

use afd_core::id::Uuid7;

use super::mint;
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
    assert_eq!(minted.expires_at_ms(), 99);
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
