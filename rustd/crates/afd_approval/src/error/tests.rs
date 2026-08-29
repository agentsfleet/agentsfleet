use std::error::Error as _;

use afd_core::error_code;

use super::{query, row_malformed};

#[test]
fn statement_and_row_failures_keep_context_and_the_postgres_cause() {
    let statement = query("resolve decision")(sqlx::Error::RowNotFound);
    assert_eq!(statement.code(), error_code::INTERNAL_OPERATION_FAILED);
    assert_eq!(
        statement.detail(),
        "The approval could not be read or recorded"
    );
    assert!(!statement.is_datastore_unavailable());
    assert!(statement.to_string().contains("resolve decision"));
    assert!(statement.source().is_some());

    let row = row_malformed("status")(sqlx::Error::ColumnNotFound("status".into()));
    assert_eq!(row.code(), error_code::INTERNAL_OPERATION_FAILED);
    assert!(row.to_string().contains("status"));
    assert!(row.source().is_some());
}
