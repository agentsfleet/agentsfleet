//! Runner error classification and source-chain contracts.

use std::error::Error as _;

use afd_core::error_code;

use super::Error;

#[test]
fn data_only_runner_failures_have_distinct_remedies() {
    let cases = [
        (
            super::runner_not_found(),
            error_code::RUNNER_NOT_FOUND,
            super::DETAIL_RUNNER_NOT_FOUND,
        ),
        (
            super::selftest_refused(),
            error_code::RUN_SELFTEST_REFUSED,
            super::DETAIL_SELFTEST_REFUSED,
        ),
        (
            super::admin_state_malformed(),
            error_code::INTERNAL_DB_QUERY,
            super::DETAIL_DATABASE_ERROR,
        ),
        (
            super::vault_data_invalid(),
            error_code::VAULT_DATA_INVALID,
            super::DETAIL_OPERATION_FAILED,
        ),
    ];
    for (error, code, detail) in &cases {
        assert_eq!(error.code(), *code);
        assert_eq!(error.detail(), *detail);
        assert!(!error.is_datastore_unavailable());
        assert!(!error.is_rejected());
        assert!(!error.is_runner_vanished());
        assert!(error.source().is_none());
    }

    let rejected = super::rejected("host_id is invalid");
    assert_eq!(rejected.code(), error_code::INVALID_REQUEST);
    assert_eq!(rejected.detail(), "host_id is invalid");
    assert!(rejected.is_rejected());
}

#[test]
fn stored_and_query_failures_retain_their_causes() -> Result<(), &'static str> {
    let malformed_id = afd_core::id::Uuid7::parse("not-a-runner")
        .err()
        .ok_or("the malformed runner fixture unexpectedly parsed")?;
    let malformed = super::row_malformed("core.runners", "id")(malformed_id);
    assert_database_error(&malformed);

    let query =
        super::query("reading runner")(sqlx::Error::Protocol("fixture runner query".into()));
    assert_database_error(&query);

    let json = serde_json::from_str::<serde_json::Value>("{")
        .err()
        .ok_or("the malformed JSON fixture unexpectedly parsed")?;
    let stored = super::stored_json("core.runners", "capability_report")(json);
    assert_database_error(&stored);
    Ok(())
}

fn assert_database_error(error: &Error) {
    assert_eq!(error.code(), error_code::INTERNAL_DB_QUERY);
    assert_eq!(error.detail(), super::DETAIL_DATABASE_ERROR);
    assert!(error.source().is_some());
    assert!(!error.to_string().is_empty());
}
