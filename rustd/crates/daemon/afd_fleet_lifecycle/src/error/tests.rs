//! Behavioural coverage for fleet-lifecycle public error answers.

use std::error::Error as _;

use af_core::error_code;

use super::{Error, ErrorKind, detail};

#[test]
fn data_only_refusals_have_stable_codes_details_and_no_causes() {
    let errors = [expected(
        ErrorKind::NotFound.into(),
        error_code::AGENTSFLEET_NOT_FOUND,
        detail::NOT_FOUND,
    )];

    for expected in &errors {
        assert_error(expected, false);
    }
}

#[test]
fn contextual_errors_preserve_context_without_leaking_it() {
    let query =
        super::query("reading fleet")(sqlx::Error::Protocol("fixture lifecycle query".into()));
    let row = super::row_malformed("status", "future-state");

    assert_error(
        &expected(query, error_code::INTERNAL_DB_QUERY, detail::DATABASE_ERROR),
        true,
    );
    assert_error(
        &expected(row, error_code::INTERNAL_DB_QUERY, detail::DATABASE_ERROR),
        false,
    );
}

struct Expected {
    error: Error,
    code: error_code::ErrorCode,
    detail: &'static str,
}

fn expected(error: Error, code: error_code::ErrorCode, detail: &'static str) -> Expected {
    Expected {
        error,
        code,
        detail,
    }
}

fn assert_error(expected: &Expected, has_source: bool) {
    assert_eq!(expected.error.code(), expected.code);
    assert_eq!(expected.error.detail(), expected.detail);
    assert!(!expected.error.is_datastore_unavailable());
    assert_eq!(expected.error.source().is_some(), has_source);
    assert!(expected.error.to_string().contains(expected.code.as_str()));
}
