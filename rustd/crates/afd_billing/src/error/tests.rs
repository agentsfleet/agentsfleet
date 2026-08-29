#![expect(
    clippy::expect_used,
    reason = "the malformed identifier is the fixture precondition"
)]

use std::error::Error as _;

use afd_core::error_code;
use afd_core::id::Uuid7;

use super::{billing_wallet_missing, charges_cursor_invalid, query, row_malformed};

#[test]
fn data_only_refusals_have_distinct_public_contracts_and_no_causes() {
    let missing = billing_wallet_missing();
    assert_eq!(missing.code(), error_code::INTERNAL_OPERATION_FAILED);
    assert!(missing.detail().contains("bootstrap invariant"));
    assert!(missing.source().is_none());

    let cursor = charges_cursor_invalid();
    assert_eq!(cursor.code(), error_code::INVALID_REQUEST);
    assert_eq!(cursor.detail(), "invalid cursor");
    assert!(cursor.source().is_none());
}

#[test]
fn query_and_row_failures_keep_the_context_only_their_raiser_knows() {
    let query = query("settle usage")(sqlx::Error::RowNotFound);
    assert_eq!(query.code(), error_code::INTERNAL_OPERATION_FAILED);
    assert!(query.to_string().contains("settle usage"));
    assert!(query.source().is_some());

    let source = Uuid7::parse("not-an-id").expect_err("fixture identifier is malformed");
    let row = row_malformed("billing.usage_ledger", "tenant_id")(source);
    assert_eq!(row.code(), error_code::INTERNAL_OPERATION_FAILED);
    assert!(row.to_string().contains("billing.usage_ledger.tenant_id"));
    assert!(row.source().is_some());
}
