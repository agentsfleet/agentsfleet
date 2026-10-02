//! Public classifications for credential failures that carry no cause.

use std::error::Error as _;

use afd_core::error_code;

use super::Error;

fn configured_kinds() -> Vec<(&'static str, Error)> {
    vec![
        ("provider field", super::provider_malformed("provider")),
        ("provider secret", super::provider_secret_missing()),
        ("platform default", super::provider_platform_key_missing()),
        ("workspace", super::provider_no_workspace()),
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
    assert!(!failure.detail().is_empty());
    assert!(failure.source().is_some());
    assert!(!failure.is_config_permanent());
    assert!(!failure.is_credential_missing());
}

#[test]
fn a_malformed_row_keeps_table_column_and_cause() -> Result<(), &'static str> {
    let source = afd_core::id::Uuid7::parse("not-an-id")
        .err()
        .ok_or("malformed fixture unexpectedly parsed")?;
    let failure = super::row_malformed("provider_selections", "tenant_id")(source);

    assert_eq!(failure.code(), error_code::INTERNAL_DB_QUERY);
    assert!(failure.to_string().contains("provider_selections"));
    assert!(failure.to_string().contains("tenant_id"));
    assert!(failure.source().is_some());
    Ok(())
}

/// A KEK hex string that will not decode, for the two crypto-sourced kinds.
///
/// Short AND non-hex, so it fails whichever check `decode_hex_into` reaches
/// first — the fixture is about producing a real `afd_crypto` error, not about
/// which of its arms produced it. Returned as a `Result` rather than unwrapped
/// because `expect_used` is denied workspace-wide, tests included.
fn crypto_failure() -> Result<afd_crypto::error::Error, &'static str> {
    afd_crypto::secret::Kek::from_hex("zz")
        .err()
        .ok_or("a two-character non-hex string unexpectedly parsed as a KEK")
}

/// An envelope that will not open keeps its cause and answers internally.
///
/// The caller cannot act on it: a stored envelope that will not decrypt is
/// this deployment's key material being wrong, not the request being wrong.
#[test]
fn a_vault_envelope_that_will_not_open_keeps_its_cause() -> Result<(), &'static str> {
    let failure = super::vault_open(crypto_failure()?);

    assert!(failure.source().is_some(), "the crypto cause is preserved");
    assert!(!failure.detail().is_empty());
    assert!(!failure.to_string().is_empty());
    assert!(!failure.is_credential_missing());
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
    assert!(!drained.detail().is_empty());
    assert!(
        !drained.is_config_permanent(),
        "a failed mint is not a stored-configuration fault an operator edits"
    );
    Ok(())
}
