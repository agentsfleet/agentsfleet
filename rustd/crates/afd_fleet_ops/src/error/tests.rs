//! Operator-projection error classification and source-chain contracts.

use std::error::Error as _;

use afd_core::error_code;

use super::Error;

#[test]
fn refusals_distinguish_missing_runners_from_bad_cursors() {
    let missing = super::runner_not_found();
    assert_error(
        &missing,
        error_code::RUNNER_NOT_FOUND,
        "Runner not found",
        false,
    );
    assert!(missing.source().is_none());

    let cursor = super::cursor_invalid();
    assert_error(
        &cursor,
        error_code::INVALID_REQUEST,
        super::super::runner_leases::DETAIL_BAD_CURSOR,
        false,
    );
    assert!(cursor.source().is_none());
}

#[test]
fn database_context_keeps_the_driver_cause() {
    let query = super::query("listing runner leases")(sqlx::Error::Protocol(
        "fixture projection query".into(),
    ));
    assert_error(
        &query,
        error_code::INTERNAL_DB_QUERY,
        "Database error",
        false,
    );
    assert!(query.source().is_some());

    let row = super::row("event_id")(sqlx::Error::Protocol("fixture projection row".into()));
    assert_error(&row, error_code::INTERNAL_DB_QUERY, "Database error", false);
    assert!(row.source().is_some());
}

fn assert_error(
    error: &Error,
    code: afd_core::error_code::ErrorCode,
    detail: &'static str,
    unavailable: bool,
) {
    assert_eq!(error.code(), code);
    assert_eq!(error.detail(), detail);
    assert_eq!(error.is_datastore_unavailable(), unavailable);
    assert!(error.to_string().contains(code.as_str()));
}
