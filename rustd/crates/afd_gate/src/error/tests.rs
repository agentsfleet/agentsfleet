//! Gate-owned refusal and query classifications.

use std::error::Error as _;

use afd_core::error_code;

use super::{Error, query, rejected};

#[test]
fn a_caller_refusal_preserves_its_sentence_and_is_not_an_outage() {
    let failure = rejected("approval mode is not supported");

    assert_eq!(failure.code(), error_code::INVALID_REQUEST);
    assert_eq!(failure.detail(), "approval mode is not supported");
    assert!(failure.is_rejected());
    assert!(!failure.is_datastore_unavailable());
    assert!(!failure.is_config_permanent());
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
    let queue = afd_redis::error::one_of_each_kind()
        .into_iter()
        .next()
        .map(|(_kind, error)| error)
        .ok_or("queue test utility exposes no error")?;
    let identifier = afd_core::id::Uuid7::parse("not-an-id")
        .err()
        .ok_or("the malformed fixture unexpectedly parsed")?;
    let (entropy, control) = afd_crypto::entropy::Entropy::new_mocked();
    control.fail_next();
    let mut bytes = [0_u8; afd_core::id::ENTROPY_LEN];
    let entropy = entropy
        .fill(&mut bytes)
        .err()
        .ok_or("the controlled entropy source unexpectedly answered")?;

    let cases = [
        (Error::from(database()?), true),
        (Error::from(queue), false),
        (Error::from(afd_credential::Error::from(database()?)), true),
        (Error::from(afd_billing::Error::from(database()?)), true),
        (Error::from(entropy), false),
        (Error::from(identifier), false),
    ];

    for (failure, unavailable) in cases {
        assert_eq!(failure.is_datastore_unavailable(), unavailable);
        assert!(!failure.code().as_str().is_empty());
        assert!(!failure.detail().is_empty());
        assert!(failure.source().is_some());
    }
    Ok(())
}
