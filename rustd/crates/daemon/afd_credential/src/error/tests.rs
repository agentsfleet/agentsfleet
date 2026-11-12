//! Public classifications for credential failures that carry no cause.

use std::error::Error as _;

use af_core::error_code;

use super::Error;

fn configured_kinds() -> Vec<(&'static str, Error)> {
    vec![
        ("provider field", super::provider_malformed("provider")),
        ("provider secret", super::provider_secret_missing()),
        ("endpoint", super::provider_endpoint("private address")),
        ("vault body", super::vault_data_invalid()),
        ("declared secret", super::credential_missing()),
    ]
}

#[test]
fn stored_configuration_failures_are_permanent_and_actionable() {
    for (label, failure) in configured_kinds() {
        assert!(failure.is_config_permanent(), "{label}");
        assert!(!failure.is_datastore_unavailable(), "{label}");
        assert!(!failure.code().as_str().is_empty(), "{label}");
        assert!(!failure.detail().is_empty(), "{label}");
        assert!(!failure.to_string().is_empty(), "{label}");
    }
}

#[test]
fn only_a_missing_declared_secret_has_the_narrow_classification() {
    for (label, failure) in configured_kinds() {
        assert_eq!(failure.is_credential_missing(), label == "declared secret");
    }
}

#[test]
fn a_query_failure_keeps_its_cause_and_internal_mapping() {
    let failure = super::query("coverage query")(sqlx::Error::PoolClosed);

    assert_eq!(failure.code(), error_code::INTERNAL_DB_QUERY);
    assert_ne!(failure.detail(), "");
    assert!(failure.source().is_some());
    assert!(!failure.is_config_permanent());
    assert!(!failure.is_credential_missing());
}

/// A KEK hex string that will not decode, for the two crypto-sourced kinds.
///
/// Short AND non-hex, so it fails whichever check `decode_hex_into` reaches
/// first — the fixture is about producing a real `af_crypto` error, not about
/// which of its arms produced it. Returned as a `Result` rather than unwrapped
/// because `expect_used` is denied workspace-wide, tests included.
fn crypto_failure() -> Result<af_crypto::error::Error, &'static str> {
    af_crypto::secret::Kek::from_hex("zz")
        .err()
        .ok_or("a two-character non-hex string unexpectedly parsed as a KEK")
}

/// A vault failure answers with the vault crate's own table.
///
/// The credential plane reads and opens through `afd_vault`, so a statement
/// that failed and an envelope that will not open must not collapse into one
/// "database error": the second is key material gone wrong, and the vault
/// already says so.
#[test]
fn a_vault_failure_answers_with_the_vault_crates_own_code() -> Result<(), &'static str> {
    let unopened = Error::from(afd_vault::Error::from(crypto_failure()?));

    assert_eq!(unopened.code(), error_code::INTERNAL_OPERATION_FAILED);
    assert_ne!(unopened.code(), error_code::INTERNAL_DB_QUERY);
    assert!(!unopened.is_datastore_unavailable());
    assert!(!unopened.is_config_permanent());
    assert!(unopened.source().is_some(), "the vault cause is preserved");
    Ok(())
}

/// An identifier that could not be minted answers the internal code.
///
/// A host that cannot draw randomness is a failure of this instance rather than
/// of its input, so it is not the caller's to correct.
#[test]
fn a_failed_mint_answers_the_internal_operation_code() -> Result<(), &'static str> {
    let drained = super::entropy_drained(crypto_failure()?);

    assert_eq!(
        drained.code(),
        error_code::INTERNAL_OPERATION_FAILED,
        "a failed mint is this instance's problem, not the caller's"
    );
    assert!(drained.source().is_some(), "a failed mint keeps its cause");
    assert_ne!(drained.detail(), "");
    assert!(
        !drained.is_config_permanent(),
        "a failed mint is not a stored-configuration fault an operator edits"
    );
    Ok(())
}
