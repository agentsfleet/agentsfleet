//! Registry mappings for every data-only tenant failure.
//!
//! These variants have no lower-level source and many represent losing races
//! that are expensive to force through a datastore. Their public contract is
//! the code, fixed detail and rendered internal diagnosis, so the table drives
//! those three methods for every variant without inventing a cause.

use super::{ApiKeyField, Error, SessionField};
use std::error::Error as _;

fn data_only_kinds() -> Vec<(&'static str, Error)> {
    let mut kinds = Vec::new();
    for field in [
        SessionField::PublicKey,
        SessionField::TokenName,
        SessionField::Ciphertext,
        SessionField::Nonce,
        SessionField::VerificationCode,
    ] {
        kinds.push(("session field", super::session_field(field)));
    }
    for field in [ApiKeyField::Name, ApiKeyField::Description] {
        kinds.push(("api-key field", super::apikey_field(field)));
    }
    kinds.extend([
        ("session missing", super::session_missing()),
        ("session expired", super::session_expired()),
        ("session consumed", super::session_consumed()),
        ("session aborted", super::session_aborted()),
        ("session rate limited", super::session_rate_limited()),
        ("session not approved", super::session_not_approved()),
        (
            "session already approved",
            super::session_already_approved(),
        ),
        ("session code rejected", super::session_code_rejected()),
        ("session not owner", super::session_not_owner()),
        ("api-key not found", super::apikey_not_found()),
        ("api-key name taken", super::apikey_name_taken()),
        ("api-key already revoked", super::apikey_already_revoked()),
        ("api-key readonly", super::apikey_readonly_field()),
        ("api-key must revoke", super::apikey_must_revoke_first()),
        ("cli machine name", super::cli_credential_machine_name()),
        ("cli credential missing", super::cli_credential_not_found()),
        (
            "cli machine collision",
            super::cli_credential_machine_collision(),
        ),
        (
            "cli subject missing",
            super::cli_credential_unknown_subject(),
        ),
        ("workspace name invalid", super::workspace_name_invalid()),
        ("workspace name long", super::workspace_name_too_long()),
        ("workspace name exists", super::workspace_name_exists()),
        (
            "workspace tenant vanished",
            super::workspace_tenant_vanished(),
        ),
    ]);
    kinds
}

#[test]
fn every_data_only_failure_has_a_registered_public_contract() {
    let kinds = data_only_kinds();
    assert_eq!(kinds.len(), 29, "the table must grow with the enum");

    for (label, failure) in kinds {
        assert!(!failure.code().as_str().is_empty(), "{label}: code");
        assert!(!failure.detail().is_empty(), "{label}: detail");
        assert!(!failure.to_string().is_empty(), "{label}: display");
        assert!(
            !failure.is_datastore_unavailable(),
            "{label}: data-only failures are not outages"
        );
    }
}

#[test]
fn field_names_render_as_the_wire_spells_them() {
    let session = [
        (SessionField::PublicKey, "public_key"),
        (SessionField::TokenName, "token_name"),
        (SessionField::Ciphertext, "ciphertext"),
        (SessionField::Nonce, "nonce"),
        (SessionField::VerificationCode, "verification_code"),
    ];
    for (field, expected) in session {
        assert_eq!(field.to_string(), expected);
    }
    assert_eq!(ApiKeyField::Name.to_string(), "key_name");
    assert_eq!(ApiKeyField::Description.to_string(), "description");
}

#[test]
fn contextual_database_failures_keep_their_classification_and_cause() -> Result<(), &'static str> {
    let query = super::query("tenant coverage query")(sqlx::Error::PoolClosed);
    assert_eq!(query.code(), afd_core::error_code::INTERNAL_DB_QUERY);
    assert!(query.source().is_some());

    let malformed_id = afd_core::id::Uuid7::parse("not-an-id")
        .err()
        .ok_or("malformed fixture unexpectedly parsed")?;
    let row = super::row_malformed("workspaces", "tenant_id")(malformed_id);
    assert_eq!(row.code(), afd_core::error_code::INTERNAL_DB_QUERY);
    assert!(row.to_string().contains("workspaces"));
    assert!(row.source().is_some());

    let page = super::library_page_unavailable(sqlx::Error::PoolClosed);
    assert_eq!(page.code(), afd_core::error_code::LIBRARY_DB_UNAVAILABLE);
    assert!(page.is_datastore_unavailable());
    assert!(page.source().is_some());
    Ok(())
}

#[test]
fn a_machine_collision_is_detectable_only_inside_the_tenant_crate() {
    let collision = super::cli_credential_machine_collision();
    assert!(collision.is_machine_collision());
    assert_eq!(
        collision.code(),
        afd_core::error_code::INTERNAL_OPERATION_FAILED
    );
}

/// A mint failure and a drawn-entropy failure lift through `From`.
///
/// Both exist so `?` can carry a foreign error across this crate's boundary
/// without a `map_err` at every call site — which is the shape
/// `docs/RUST_ERROR_STANDARD.md` requires, and the shape that keeps the
/// `source()` chain intact. What the test holds is exactly that: the lift
/// happens AND the cause survives it.
#[test]
fn foreign_failures_lift_through_from_and_keep_their_cause() -> Result<(), &'static str> {
    use std::error::Error as _;

    let minted: super::Error = afd_core::id::Uuid7::parse("not-an-id")
        .err()
        .ok_or("a malformed identifier unexpectedly parsed")?
        .into();
    let drawn: super::Error = afd_crypto::secret::Kek::from_hex("zz")
        .err()
        .ok_or("a two-character non-hex string unexpectedly parsed as a KEK")?
        .into();

    for (label, failure) in [("mint", &minted), ("entropy", &drawn)] {
        assert!(failure.source().is_some(), "{label} keeps its cause");
        assert!(!failure.to_string().is_empty(), "{label}");
        assert!(!failure.code().as_str().is_empty(), "{label}");
    }
    Ok(())
}
