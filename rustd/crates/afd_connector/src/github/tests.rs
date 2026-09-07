//! Parse and handle-shape tests for [`super`].
//!
//! Included by `#[path]` from the module root rather than sitting inside it:
//! the file was over the length cap with them inline, and these cover the pure
//! half, which is the half that can be tested without a vendor at all.
#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use serde_json::json;

use super::{
    Found, HANDLE_INSTALLATION_ID, Installation, access_token, grant, is_installation_id,
    parse_listing,
};
use crate::grant::InstallClaim;

/// One listed installation is the one to bind, id and account read out.
#[test]
fn one_listed_installation_binds_with_its_account_as_the_label() {
    let listing = json!({"installations": [{"id": 424_242, "account": {"login": "acme"}}]});
    assert_eq!(
        parse_listing(&listing),
        Some(Found::One(Installation {
            id: "424242".into(),
            account: Some("acme".into()),
        }))
    );
}

/// None and several are refusals, not choices.
#[test]
fn an_empty_listing_and_a_listing_of_two_both_bind_nothing() {
    assert_eq!(
        parse_listing(&json!({"installations": []})),
        Some(Found::None)
    );
    assert_eq!(
        parse_listing(&json!({"installations": [{"id": 1}, {"id": 2}]})),
        Some(Found::Several)
    );
}

/// A body this build cannot read as a listing is unreadable, never a bind.
#[test]
fn an_unreadable_listing_binds_nothing_and_says_so() {
    for body in [
        json!({}),
        json!({"installations": "not-a-list"}),
        json!({"installations": [{"login": "acme"}]}),
        json!({"installations": [{"id": -1}]}),
        json!({"installations": [{"id": "not-a-number"}]}),
    ] {
        assert_eq!(parse_listing(&body), None, "`{body}` is unreadable");
    }
}

/// A decimal-string id is accepted the same as a number.
#[test]
fn a_decimal_string_id_reads_as_the_same_installation() {
    let listing = json!({"installations": [{"id": "7"}]});
    assert!(matches!(parse_listing(&listing), Some(Found::One(found)) if found.id == "7"));
}

/// The shape rule the callback applies to a claimed id.
#[test]
fn an_installation_id_is_bounded_decimal_digits() {
    assert!(is_installation_id("12345678"));
    for bad in ["", "12a45", "-1", &"1".repeat(33)] {
        assert!(
            !is_installation_id(bad),
            "`{bad}` is not an installation id"
        );
    }
}

/// The handle carries what the broker mints from and the row is exclusive.
#[test]
fn the_grant_carries_the_installation_and_claims_it_exclusively() {
    let landed = grant(
        &Installation {
            id: "99".into(),
            account: Some("acme".into()),
        },
        1_700_000_000_000,
    );
    assert_eq!(
        landed
            .handle
            .get(HANDLE_INSTALLATION_ID)
            .and_then(|id| id.as_str()),
        Some("99")
    );
    assert_eq!(
        landed.handle.get("label").and_then(|label| label.as_str()),
        Some("acme")
    );
    let install = landed.install.expect("a GitHub grant routes");
    assert_eq!(install.external_account_id, "99");
    assert_eq!(install.claim, InstallClaim::Exclusive);
}

/// The token is the one field, and an empty one is no token.
#[test]
fn the_user_token_is_read_out_of_the_exchange_answer() {
    assert_eq!(
        access_token(&json!({"access_token": "gho_x", "token_type": "bearer"})).as_deref(),
        Some("gho_x")
    );
    for body in [
        json!({}),
        json!({"access_token": ""}),
        json!({"access_token": 1}),
    ] {
        assert_eq!(access_token(&body), None, "`{body}` carries no token");
    }
}

/// Every outcome names itself for the operator's log.
#[test]
fn every_outcome_has_a_reason() {
    let one = Found::One(Installation {
        id: "1".into(),
        account: None,
    });
    for (found, reason) in [
        (Found::None, "no_accessible_installation"),
        (one, "one_installation"),
        (Found::Several, "several_installations"),
    ] {
        assert_eq!(found.reason(), reason);
    }
}
