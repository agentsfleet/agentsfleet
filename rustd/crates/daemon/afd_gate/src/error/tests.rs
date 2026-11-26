//! Gate-owned refusal and query classifications.

use std::error::Error as _;

use af_core::error_code;

use super::{Error, query, rejected};

#[test]
fn a_caller_refusal_preserves_its_sentence_and_is_not_an_outage() {
    let failure = rejected("approval mode is not supported");

    assert_eq!(failure.code(), error_code::INVALID_REQUEST);
    assert_eq!(failure.detail(), "approval mode is not supported");
    assert!(failure.is_rejected());
    assert!(!failure.is_datastore_unavailable());
    assert!(failure.source().is_none());
}

#[test]
fn a_query_failure_keeps_the_database_cause() {
    let failure: Error = query("gate coverage query")(sqlx::Error::PoolClosed);

    assert_eq!(failure.code(), error_code::INTERNAL_DB_QUERY);
    assert!(!failure.is_rejected());
    assert!(!failure.is_datastore_unavailable());
    assert!(failure.source().is_some());
}

#[test]
fn composed_failures_delegate_classification_and_keep_causes() -> Result<(), &'static str> {
    let database = || {
        afd_db::error::one_of_each_kind()
            .into_iter()
            .find(|(kind, _error)| *kind == "datastore unavailable")
            .map(|(_kind, error)| error)
            .ok_or("database test utility has no outage kind")
    };
    let scope = af_fleet_runtime::config::RepositoryBinding::parse("{}")
        .err()
        .ok_or("an empty scope unexpectedly parsed")?;

    // The third column is whether the gate is a NEW causal link. Every variant
    // here wraps a failure that happened in another crate, and every one adds
    // the fact that crate cannot carry: WHICH plane was using it. Only a
    // datastore failure is an availability signal, which is the second column.
    let cases = [
        ("database", Error::from(database()?), true, true),
        (
            "credential",
            Error::from(afd_credential::Error::from(database()?)),
            true,
            true,
        ),
        ("scope", Error::from(scope), false, true),
    ];

    for (name, failure, unavailable, has_cause) in cases {
        assert_eq!(failure.is_datastore_unavailable(), unavailable, "{name}");
        assert!(!failure.code().as_str().is_empty(), "{name}");
        assert!(!failure.detail().is_empty(), "{name}");
        assert_eq!(
            failure.source().is_some(),
            has_cause,
            "{name} disagrees with its declared causality"
        );
    }
    Ok(())
}

#[test]
fn a_credential_failure_is_classed_by_the_credential_plane() -> Result<(), &'static str> {
    // A database outage behind a credential read recovers on its own, so the
    // gate must not report it as a stored endpoint that will never work.
    let outage = afd_db::error::one_of_each_kind()
        .into_iter()
        .find(|(kind, _error)| *kind == "datastore unavailable")
        .map(|(_kind, error)| error)
        .ok_or("database test utility has no outage kind")?;
    let failure = Error::from(afd_credential::Error::from(outage));

    assert!(failure.is_datastore_unavailable());
    Ok(())
}
