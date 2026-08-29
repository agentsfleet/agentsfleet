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
