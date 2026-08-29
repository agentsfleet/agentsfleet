//! Behavioural coverage for administration error boundaries.

use std::error::Error as _;

use afd_core::error_code;

use super::{Error, row};

#[test]
fn query_and_row_failures_keep_causes_behind_safe_details() -> Result<(), &'static str> {
    let query = super::query("listing models")(sqlx::Error::Protocol(
        "fixture administration query".into(),
    ));
    assert_error(&query, error_code::INTERNAL_DB_QUERY, "Database error");

    let malformed_id = afd_core::id::Uuid7::parse("not-an-id")
        .err()
        .ok_or("the malformed identifier fixture unexpectedly parsed")?;
    let malformed = row("core.models", "id")(malformed_id);
    assert_error(&malformed, error_code::INTERNAL_DB_QUERY, "Database error");

    Ok(())
}

#[test]
fn mint_failures_are_internal_and_retain_the_domain_cause() -> Result<(), &'static str> {
    let malformed_id = afd_core::id::Uuid7::parse("still-not-an-id")
        .err()
        .ok_or("the malformed identifier fixture unexpectedly parsed")?;
    let mint = Error::from(malformed_id);
    assert_error(
        &mint,
        error_code::INTERNAL_OPERATION_FAILED,
        "Internal operation failed",
    );
    Ok(())
}

fn assert_error(error: &Error, code: afd_core::error_code::ErrorCode, detail: &'static str) {
    assert_eq!(error.code(), code);
    assert_eq!(error.detail(), detail);
    assert!(!error.is_datastore_unavailable());
    assert!(error.source().is_some());
    assert!(error.to_string().contains(code.as_str()));
}
