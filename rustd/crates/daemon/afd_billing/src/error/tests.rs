#![expect(
    clippy::expect_used,
    reason = "the malformed identifier is the fixture precondition"
)]

use std::error::Error as _;

use af_core::error_code;
use af_core::id::Uuid7;

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
    let row = row_malformed("billing.usage_lines", "tenant_id")(source);
    assert_eq!(row.code(), error_code::INTERNAL_OPERATION_FAILED);
    assert!(row.to_string().contains("billing.usage_lines.tenant_id"));
    assert!(row.source().is_some());
}

/// Only a query the datastore answered "not now" is contention; a refusal
/// that never reached a statement is not.
#[test]
fn a_refusal_with_no_query_is_never_contended() {
    assert!(!billing_wallet_missing().is_contended());
    assert!(!charges_cursor_invalid().is_contended());
}

/// The login ends a lock wait or a long statement with these two SQLSTATEs.
/// The datastore answered "not now", so the report and renewal paths that run
/// `log_lines` tell the runner to retry (503) rather than counting a defect
/// (500) toward its self-termination ceiling. It stays contention, not an
/// outage, for the outage policy. Gated on `test-util`, which is what unlocks
/// a Postgres answer built without a server.
#[cfg(feature = "test-util")]
#[test]
fn a_statement_ended_by_the_logins_bound_answers_unavailable() {
    for sqlstate in ["55P03", "57014"] {
        let fault = query("settle usage lines")(afd_db::error::database_answer(sqlstate));

        assert_eq!(
            fault.code(),
            error_code::INTERNAL_DB_UNAVAILABLE,
            "{sqlstate}"
        );
        assert_eq!(fault.detail(), super::DETAIL_UNAVAILABLE, "{sqlstate}");
        assert!(fault.is_contended(), "{sqlstate}");
        assert!(fault.is_unanswered(), "{sqlstate}");
        assert!(!fault.is_datastore_unavailable(), "{sqlstate}");
        assert!(fault.to_string().contains("settle usage lines"), "{fault}");
        assert!(fault.source().is_some(), "{sqlstate} keeps its cause");
    }
}

/// Only the login's two bounds read as "try again"; a constraint or a deadlock
/// is a defect in the statement and keeps its operation-failed answer.
#[cfg(feature = "test-util")]
#[test]
fn a_statement_refused_for_any_other_reason_stays_an_operation_failure() {
    for sqlstate in ["23505", "40P01"] {
        let fault = query("settle usage lines")(afd_db::error::database_answer(sqlstate));

        assert_eq!(
            fault.code(),
            error_code::INTERNAL_OPERATION_FAILED,
            "{sqlstate}"
        );
        assert_eq!(fault.detail(), super::DETAIL_OPERATION_FAILED, "{sqlstate}");
        assert!(!fault.is_contended(), "{sqlstate}");
        assert!(!fault.is_unanswered(), "{sqlstate}");
    }
}
