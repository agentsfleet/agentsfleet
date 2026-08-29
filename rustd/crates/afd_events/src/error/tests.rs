use std::error::Error as _;

use afd_core::error_code;

use super::{Error, query, row_malformed};

#[test]
fn malformed_cursors_are_caller_faults_without_invented_causes() {
    let error = Error::CursorMalformed;
    assert_eq!(error.code(), error_code::INVALID_REQUEST);
    assert_eq!(error.detail(), "The cursor is not valid");
    assert!(!error.is_datastore_unavailable());
    assert!(error.source().is_none());
}

#[test]
fn query_and_row_failures_preserve_their_context_and_causes() {
    let query = query("page fleet history")(sqlx::Error::RowNotFound);
    assert_eq!(query.code(), error_code::INTERNAL_DB_QUERY);
    assert_eq!(query.detail(), "Database error");
    assert!(query.to_string().contains("page fleet history"));
    assert!(query.source().is_some());

    let row = row_malformed("event_type")(sqlx::Error::ColumnNotFound("event_type".into()));
    assert_eq!(row.code(), error_code::INTERNAL_DB_QUERY);
    assert!(row.to_string().contains("event_type"));
    assert!(row.source().is_some());
}
