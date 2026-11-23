//! Runner error classification and source-chain contracts.

use std::error::Error as _;

use af_core::error_code;

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
            super::assignment_malformed("sandbox_tier"),
            error_code::INTERNAL_DB_QUERY,
            super::DETAIL_DATABASE_ERROR,
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
    assert!(rejected.registered_runner().is_none());
}

/// A second registration of one host is a conflict that names the runner the
/// host already is, so the caller recovers by reading it.
#[test]
fn a_registered_host_names_its_runner() -> Result<(), af_core::error::Error> {
    let runner = af_core::id::Uuid7::parse("0199a801-1000-7000-8000-000000000001")?;
    let error = super::host_registered(runner.clone());

    assert_eq!(error.code(), error_code::RUNNER_HOST_REGISTERED);
    assert_eq!(error.detail(), super::DETAIL_HOST_REGISTERED);
    assert_eq!(error.registered_runner(), Some(&runner));
    assert!(!error.is_rejected());
    assert!(!error.is_datastore_unavailable());
    assert!(error.to_string().contains(runner.as_str()), "{error}");
    Ok(())
}

#[test]
fn stored_and_query_failures_retain_their_causes() -> Result<(), &'static str> {
    let malformed_id = af_core::id::Uuid7::parse("not-a-runner")
        .err()
        .ok_or("the malformed runner fixture unexpectedly parsed")?;
    let malformed = super::row_malformed("runner.runners", "id")(malformed_id);
    assert_database_error(&malformed);

    let query =
        super::query("reading runner")(sqlx::Error::Protocol("fixture runner query".into()));
    assert_database_error(&query);

    let json = serde_json::from_str::<serde_json::Value>("{")
        .err()
        .ok_or("the malformed JSON fixture unexpectedly parsed")?;
    let stored = super::stored_json("runner.runners", "capability_report")(json);
    assert_database_error(&stored);
    Ok(())
}

/// The API login ends a lock wait or a long statement with these two
/// SQLSTATEs. The datastore answered "not now", so the runner is told to retry
/// instead of being handed a failure its client counts toward self-termination.
#[test]
fn a_statement_ended_by_the_logins_bound_is_unavailable_and_retryable() {
    for sqlstate in ["55P03", "57014"] {
        let error = super::query("lock and count host run capacity")(
            afd_db::error::database_answer(sqlstate),
        );

        assert_eq!(
            error.code(),
            error_code::INTERNAL_DB_UNAVAILABLE,
            "{sqlstate}"
        );
        assert_eq!(error.detail(), super::DETAIL_UNAVAILABLE, "{sqlstate}");
        assert!(error.is_datastore_unavailable(), "{sqlstate}");
        assert!(!error.is_rejected(), "{sqlstate}");
        assert!(!error.is_runner_vanished(), "{sqlstate}");
        assert!(
            error
                .to_string()
                .contains("lock and count host run capacity"),
            "{error}"
        );
        assert!(error.source().is_some(), "{sqlstate} keeps its cause");
    }
}

/// A statement the datastore refused on its own terms is still a defect: only
/// the login's two bounds read as "try again".
#[test]
fn a_statement_refused_for_any_other_reason_stays_a_defect() {
    for sqlstate in ["23505", "40P01"] {
        let error = super::query("reading runner")(afd_db::error::database_answer(sqlstate));

        assert_eq!(error.code(), error_code::INTERNAL_DB_QUERY, "{sqlstate}");
        assert_eq!(error.detail(), super::DETAIL_DATABASE_ERROR, "{sqlstate}");
        assert!(!error.is_datastore_unavailable(), "{sqlstate}");
    }
}

fn assert_database_error(error: &Error) {
    assert_eq!(error.code(), error_code::INTERNAL_DB_QUERY);
    assert_eq!(error.detail(), super::DETAIL_DATABASE_ERROR);
    assert!(error.source().is_some());
    assert!(!error.to_string().is_empty(), "{error}");
}
